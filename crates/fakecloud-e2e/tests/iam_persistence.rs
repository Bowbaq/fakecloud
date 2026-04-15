mod helpers;

use std::collections::HashMap;

use aws_sdk_iam::types::Tag;
use helpers::TestServer;

/// Round-trip: users, roles, groups, managed + inline policies, access
/// keys, tags, account alias, instance profile, OIDC provider, password
/// policy all survive a server restart.
#[tokio::test]
async fn persistence_round_trip_iam_entities() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.iam_client().await;

    // User with tags + permissions boundary candidate policy.
    client
        .create_user()
        .user_name("alice")
        .path("/engineering/")
        .tags(
            Tag::builder()
                .key("team")
                .value("platform")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // Access key for alice — this is the thing auth will need after restart.
    let ak = client
        .create_access_key()
        .user_name("alice")
        .send()
        .await
        .unwrap();
    let ak_id = ak.access_key().unwrap().access_key_id().to_string();

    // Managed policy.
    let policy_doc = r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"s3:GetObject","Resource":"*"}]}"#;
    let policy = client
        .create_policy()
        .policy_name("ReadS3")
        .policy_document(policy_doc)
        .description("read-only S3")
        .send()
        .await
        .unwrap();
    let policy_arn = policy.policy().unwrap().arn().unwrap().to_string();

    client
        .attach_user_policy()
        .user_name("alice")
        .policy_arn(&policy_arn)
        .send()
        .await
        .unwrap();

    // Inline policy on the user.
    client
        .put_user_policy()
        .user_name("alice")
        .policy_name("inline-deny-delete")
        .policy_document(
            r#"{"Version":"2012-10-17","Statement":[{"Effect":"Deny","Action":"s3:DeleteObject","Resource":"*"}]}"#,
        )
        .send()
        .await
        .unwrap();

    // Role with assume-role trust policy.
    let trust = r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"lambda.amazonaws.com"},"Action":"sts:AssumeRole"}]}"#;
    client
        .create_role()
        .role_name("lambda-exec")
        .assume_role_policy_document(trust)
        .description("lambda execution role")
        .max_session_duration(7200)
        .send()
        .await
        .unwrap();
    client
        .attach_role_policy()
        .role_name("lambda-exec")
        .policy_arn(&policy_arn)
        .send()
        .await
        .unwrap();

    // Group with a member + inline policy.
    client
        .create_group()
        .group_name("devs")
        .send()
        .await
        .unwrap();
    client
        .add_user_to_group()
        .group_name("devs")
        .user_name("alice")
        .send()
        .await
        .unwrap();
    client
        .put_group_policy()
        .group_name("devs")
        .policy_name("group-inline")
        .policy_document(
            r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"sqs:*","Resource":"*"}]}"#,
        )
        .send()
        .await
        .unwrap();

    // Instance profile referencing the role.
    client
        .create_instance_profile()
        .instance_profile_name("lambda-profile")
        .send()
        .await
        .unwrap();
    client
        .add_role_to_instance_profile()
        .instance_profile_name("lambda-profile")
        .role_name("lambda-exec")
        .send()
        .await
        .unwrap();

    // Account alias.
    client
        .create_account_alias()
        .account_alias("fake-corp")
        .send()
        .await
        .unwrap();

    // Restart — everything above must survive reading the snapshot.
    server.restart().await;
    let client = server.iam_client().await;

    // User + tags.
    let got = client.get_user().user_name("alice").send().await.unwrap();
    let user = got.user().unwrap();
    assert_eq!(user.user_name(), "alice");
    assert_eq!(user.path(), "/engineering/");
    let tag_map: HashMap<String, String> = user
        .tags()
        .iter()
        .map(|t| (t.key().to_string(), t.value().to_string()))
        .collect();
    assert_eq!(tag_map.get("team").map(String::as_str), Some("platform"));

    // Access key persisted with the same AKID.
    let keys = client
        .list_access_keys()
        .user_name("alice")
        .send()
        .await
        .unwrap();
    let akids: Vec<&str> = keys
        .access_key_metadata()
        .iter()
        .map(|m| m.access_key_id().unwrap_or_default())
        .collect();
    assert!(akids.contains(&ak_id.as_str()));

    // Managed policy survives and is still attached to the user.
    client
        .get_policy()
        .policy_arn(&policy_arn)
        .send()
        .await
        .unwrap();
    let attached = client
        .list_attached_user_policies()
        .user_name("alice")
        .send()
        .await
        .unwrap();
    let attached_arns: Vec<&str> = attached
        .attached_policies()
        .iter()
        .map(|p| p.policy_arn().unwrap_or_default())
        .collect();
    assert!(attached_arns.contains(&policy_arn.as_str()));

    // Inline user policy survives.
    let inline = client
        .get_user_policy()
        .user_name("alice")
        .policy_name("inline-deny-delete")
        .send()
        .await
        .unwrap();
    // IAM URL-encodes policy documents in GET responses, so match on
    // the percent-encoded form as well as the raw form to be robust.
    let doc = inline.policy_document();
    assert!(
        doc.contains("s3:DeleteObject") || doc.contains("s3%3ADeleteObject"),
        "inline policy document did not survive restart: {doc}",
    );

    // Role + attached policy.
    let role = client
        .get_role()
        .role_name("lambda-exec")
        .send()
        .await
        .unwrap();
    assert_eq!(role.role().unwrap().max_session_duration(), Some(7200),);
    let role_attached = client
        .list_attached_role_policies()
        .role_name("lambda-exec")
        .send()
        .await
        .unwrap();
    assert!(role_attached
        .attached_policies()
        .iter()
        .any(|p| p.policy_arn() == Some(policy_arn.as_str())));

    // Group with member + inline policy survives.
    let group = client.get_group().group_name("devs").send().await.unwrap();
    let member_names: Vec<&str> = group.users().iter().map(|u| u.user_name()).collect();
    assert!(member_names.contains(&"alice"));
    let group_inline = client
        .get_group_policy()
        .group_name("devs")
        .policy_name("group-inline")
        .send()
        .await
        .unwrap();
    let gdoc = group_inline.policy_document();
    assert!(
        gdoc.contains("sqs:*") || gdoc.contains("sqs%3A*") || gdoc.contains("sqs"),
        "group inline policy did not survive restart: {gdoc}",
    );

    // Instance profile + role membership.
    let profile = client
        .get_instance_profile()
        .instance_profile_name("lambda-profile")
        .send()
        .await
        .unwrap();
    let profile_role_names: Vec<&str> = profile
        .instance_profile()
        .unwrap()
        .roles()
        .iter()
        .map(|r| r.role_name())
        .collect();
    assert!(profile_role_names.contains(&"lambda-exec"));

    // Account alias.
    let aliases = client.list_account_aliases().send().await.unwrap();
    let a: Vec<&str> = aliases
        .account_aliases()
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert!(a.contains(&"fake-corp"));

    // Mutations after restart still persist.
    client.create_user().user_name("bob").send().await.unwrap();
    server.restart().await;
    let client = server.iam_client().await;
    client.get_user().user_name("bob").send().await.unwrap();
}

/// STS temporary credentials issued before a restart must still resolve
/// after the restart — otherwise callers that already hold a session
/// token would be silently rejected.
#[tokio::test]
async fn persistence_sts_temp_credentials_survive_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;

    // Create a role to assume.
    let iam = server.iam_client().await;
    let trust = r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"AWS":"*"},"Action":"sts:AssumeRole"}]}"#;
    iam.create_role()
        .role_name("target-role")
        .assume_role_policy_document(trust)
        .send()
        .await
        .unwrap();

    let sts = server.sts_client().await;
    let resp = sts
        .assume_role()
        .role_arn("arn:aws:iam::123456789012:role/target-role")
        .role_session_name("session-1")
        .duration_seconds(3600)
        .send()
        .await
        .unwrap();
    let creds = resp.credentials().unwrap();
    let akid = creds.access_key_id().to_string();
    let secret = creds.secret_access_key().to_string();

    server.restart().await;
    let iam = server.iam_client().await;

    // The persisted role still exists (same assertion as batch 1 above).
    iam.get_role()
        .role_name("target-role")
        .send()
        .await
        .unwrap();

    // After restart, looking up GetAccessKeyInfo for the issued AKID
    // should still succeed. That only works if sts_temp_credentials
    // round-tripped through the snapshot.
    let sts = server.sts_client().await;
    let info = sts
        .get_access_key_info()
        .access_key_id(&akid)
        .send()
        .await
        .unwrap();
    // Account id is included in the response and must match the account
    // the server is configured with — this proves the temp credential
    // was looked up by AKID from the restored snapshot.
    assert_eq!(info.account(), Some("123456789012"));
    // Silence the unused-var warning — secret is meaningful to the test
    // narrative even though we can't re-sign requests from here.
    let _ = secret;
}
