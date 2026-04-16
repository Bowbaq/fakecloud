mod helpers;

use helpers::TestServer;

/// Parameter groups survive a restart.
#[tokio::test]
async fn persistence_round_trip_parameter_group() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.rds_client().await;

    client
        .create_db_parameter_group()
        .db_parameter_group_name("persist-pg")
        .db_parameter_group_family("postgres16")
        .description("Persistence test parameter group")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.rds_client().await;

    let groups = client
        .describe_db_parameter_groups()
        .db_parameter_group_name("persist-pg")
        .send()
        .await
        .unwrap();
    let pgs = groups.db_parameter_groups();
    assert!(
        pgs.iter()
            .any(|g| g.db_parameter_group_name() == Some("persist-pg")),
        "parameter group should survive restart"
    );
    let pg = pgs
        .iter()
        .find(|g| g.db_parameter_group_name() == Some("persist-pg"))
        .unwrap();
    assert_eq!(pg.db_parameter_group_family(), Some("postgres16"));
    assert_eq!(pg.description(), Some("Persistence test parameter group"));
}

/// Subnet groups survive a restart.
#[tokio::test]
async fn persistence_round_trip_subnet_group() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.rds_client().await;

    client
        .create_db_subnet_group()
        .db_subnet_group_name("persist-subnet-grp")
        .db_subnet_group_description("Persistence test subnet group")
        .subnet_ids("subnet-aaa")
        .subnet_ids("subnet-bbb")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.rds_client().await;

    let groups = client
        .describe_db_subnet_groups()
        .db_subnet_group_name("persist-subnet-grp")
        .send()
        .await
        .unwrap();
    let sgs = groups.db_subnet_groups();
    assert_eq!(sgs.len(), 1);
    let sg = &sgs[0];
    assert_eq!(sg.db_subnet_group_name(), Some("persist-subnet-grp"));
    assert_eq!(
        sg.db_subnet_group_description(),
        Some("Persistence test subnet group")
    );
    let subnet_ids: Vec<&str> = sg
        .subnets()
        .iter()
        .filter_map(|s| s.subnet_identifier())
        .collect();
    assert!(subnet_ids.contains(&"subnet-aaa"));
    assert!(subnet_ids.contains(&"subnet-bbb"));
}

/// Deletion survives a restart: a deleted parameter group does not reappear.
#[tokio::test]
async fn persistence_deletion_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.rds_client().await;

    client
        .create_db_parameter_group()
        .db_parameter_group_name("doomed-pg")
        .db_parameter_group_family("postgres16")
        .description("Will be deleted")
        .send()
        .await
        .unwrap();

    client
        .delete_db_parameter_group()
        .db_parameter_group_name("doomed-pg")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.rds_client().await;

    let groups = client.describe_db_parameter_groups().send().await.unwrap();
    assert!(
        !groups
            .db_parameter_groups()
            .iter()
            .any(|g| g.db_parameter_group_name() == Some("doomed-pg")),
        "deleted parameter group should not reappear"
    );
}
