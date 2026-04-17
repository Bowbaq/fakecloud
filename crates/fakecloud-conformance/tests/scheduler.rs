mod helpers;

use aws_sdk_scheduler::types::{
    ActionAfterCompletion, FlexibleTimeWindow, FlexibleTimeWindowMode, ScheduleState, Target,
};
use fakecloud_conformance_macros::test_action;
use helpers::TestServer;

fn sqs_target() -> Target {
    Target::builder()
        .arn("arn:aws:sqs:us-east-1:000000000000:conf-dest")
        .role_arn("arn:aws:iam::000000000000:role/scheduler")
        .input("{}")
        .build()
        .unwrap()
}

fn off_window() -> FlexibleTimeWindow {
    FlexibleTimeWindow::builder()
        .mode(FlexibleTimeWindowMode::Off)
        .build()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Schedule lifecycle
// ---------------------------------------------------------------------------

#[test_action("scheduler", "CreateSchedule", checksum = "e6144949")]
#[tokio::test]
async fn scheduler_create_schedule() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    let resp = client
        .create_schedule()
        .name("conf-s1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    assert!(resp.schedule_arn().contains("conf-s1"));
}

#[test_action("scheduler", "GetSchedule", checksum = "3402bb24")]
#[tokio::test]
async fn scheduler_get_schedule() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule()
        .name("conf-g1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    let resp = client.get_schedule().name("conf-g1").send().await.unwrap();
    assert_eq!(resp.name().unwrap(), "conf-g1");
}

#[test_action("scheduler", "UpdateSchedule", checksum = "d088c0c3")]
#[tokio::test]
async fn scheduler_update_schedule() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule()
        .name("conf-u1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    client
        .update_schedule()
        .name("conf-u1")
        .schedule_expression("rate(5 minutes)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .state(ScheduleState::Disabled)
        .send()
        .await
        .unwrap();
    let got = client.get_schedule().name("conf-u1").send().await.unwrap();
    assert_eq!(got.schedule_expression().unwrap(), "rate(5 minutes)");
    assert_eq!(got.state().unwrap(), &ScheduleState::Disabled);
}

#[test_action("scheduler", "DeleteSchedule", checksum = "7af6e0e4")]
#[tokio::test]
async fn scheduler_delete_schedule() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule()
        .name("conf-d1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    client
        .delete_schedule()
        .name("conf-d1")
        .send()
        .await
        .unwrap();
}

#[test_action("scheduler", "ListSchedules", checksum = "7ebcd709")]
#[tokio::test]
async fn scheduler_list_schedules() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule()
        .name("conf-l1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    let resp = client.list_schedules().send().await.unwrap();
    assert!(resp.schedules().iter().any(|s| s.name() == Some("conf-l1")));
}

// ---------------------------------------------------------------------------
// Schedule group lifecycle
// ---------------------------------------------------------------------------

#[test_action("scheduler", "CreateScheduleGroup", checksum = "ac530c8c")]
#[tokio::test]
async fn scheduler_create_schedule_group() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    let resp = client
        .create_schedule_group()
        .name("conf-grp-create")
        .send()
        .await
        .unwrap();
    assert!(resp.schedule_group_arn().contains("conf-grp-create"));
}

#[test_action("scheduler", "GetScheduleGroup", checksum = "bc3f961d")]
#[tokio::test]
async fn scheduler_get_schedule_group() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    let resp = client
        .get_schedule_group()
        .name("default")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.name().unwrap(), "default");
}

#[test_action("scheduler", "DeleteScheduleGroup", checksum = "5361c451")]
#[tokio::test]
async fn scheduler_delete_schedule_group() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule_group()
        .name("conf-grp-delete")
        .send()
        .await
        .unwrap();
    client
        .delete_schedule_group()
        .name("conf-grp-delete")
        .send()
        .await
        .unwrap();
}

#[test_action("scheduler", "ListScheduleGroups", checksum = "bd3d99ce")]
#[tokio::test]
async fn scheduler_list_schedule_groups() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    let resp = client.list_schedule_groups().send().await.unwrap();
    assert!(resp
        .schedule_groups()
        .iter()
        .any(|g| g.name() == Some("default")));
}

// ---------------------------------------------------------------------------
// Tagging (targets the default schedule group)
// ---------------------------------------------------------------------------

#[test_action("scheduler", "TagResource", checksum = "0b1f8216")]
#[tokio::test]
async fn scheduler_tag_resource() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule_group()
        .name("conf-tag")
        .send()
        .await
        .unwrap();
    let arn = "arn:aws:scheduler:us-east-1:000000000000:schedule-group/conf-tag";
    client
        .tag_resource()
        .resource_arn(arn)
        .tags(
            aws_sdk_scheduler::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();
}

#[test_action("scheduler", "UntagResource", checksum = "ae95fe3b")]
#[tokio::test]
async fn scheduler_untag_resource() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule_group()
        .name("conf-untag")
        .send()
        .await
        .unwrap();
    let arn = "arn:aws:scheduler:us-east-1:000000000000:schedule-group/conf-untag";
    client
        .tag_resource()
        .resource_arn(arn)
        .tags(
            aws_sdk_scheduler::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();
    client
        .untag_resource()
        .resource_arn(arn)
        .tag_keys("env")
        .send()
        .await
        .unwrap();
}

#[test_action("scheduler", "ListTagsForResource", checksum = "ec2ba4ef")]
#[tokio::test]
async fn scheduler_list_tags_for_resource() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule_group()
        .name("conf-tags")
        .send()
        .await
        .unwrap();
    let arn = "arn:aws:scheduler:us-east-1:000000000000:schedule-group/conf-tags";
    client
        .tag_resource()
        .resource_arn(arn)
        .tags(
            aws_sdk_scheduler::types::Tag::builder()
                .key("a")
                .value("1")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let resp = client
        .list_tags_for_resource()
        .resource_arn(arn)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.tags().len(), 1);
}

// ---------------------------------------------------------------------------
// At-expression + ActionAfterCompletion persistence (smoke round-trip)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scheduler_at_schedule_with_action_after_completion() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;
    client
        .create_schedule()
        .name("conf-at")
        .schedule_expression("at(2099-01-01T12:00:00)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .action_after_completion(ActionAfterCompletion::Delete)
        .send()
        .await
        .unwrap();
    let got = client.get_schedule().name("conf-at").send().await.unwrap();
    assert_eq!(
        got.action_after_completion().unwrap(),
        &ActionAfterCompletion::Delete
    );
}
