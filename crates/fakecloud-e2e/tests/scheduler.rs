//! End-to-end tests for EventBridge Scheduler (`scheduler.amazonaws.com`).
//!
//! Batch 1: CRUD round-trips against the AWS SDK. Firing / DLQ /
//! self-delete tests land in Batch 2.

mod helpers;

use aws_sdk_scheduler::types::{
    ActionAfterCompletion, FlexibleTimeWindow, FlexibleTimeWindowMode, ScheduleState, Target,
};
use fakecloud_testkit::TestServer;

fn sqs_target() -> Target {
    Target::builder()
        .arn("arn:aws:sqs:us-east-1:000000000000:scheduler-dest")
        .role_arn("arn:aws:iam::000000000000:role/scheduler")
        .input("{\"hello\":\"world\"}")
        .build()
        .unwrap()
}

fn off_window() -> FlexibleTimeWindow {
    FlexibleTimeWindow::builder()
        .mode(FlexibleTimeWindowMode::Off)
        .build()
        .unwrap()
}

#[tokio::test]
async fn scheduler_create_get_delete_schedule() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;

    let created = client
        .create_schedule()
        .name("crud-s1")
        .schedule_expression("rate(5 minutes)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .expect("create_schedule");
    assert!(created.schedule_arn().contains("schedule/default/crud-s1"));

    let got = client
        .get_schedule()
        .name("crud-s1")
        .send()
        .await
        .expect("get_schedule");
    assert_eq!(got.name().unwrap(), "crud-s1");
    assert_eq!(got.group_name().unwrap(), "default");
    assert_eq!(got.schedule_expression().unwrap(), "rate(5 minutes)");
    assert_eq!(got.state().unwrap(), &ScheduleState::Enabled);
    let target = got.target().unwrap();
    assert_eq!(target.input().unwrap(), "{\"hello\":\"world\"}");

    client
        .delete_schedule()
        .name("crud-s1")
        .send()
        .await
        .expect("delete_schedule");

    let err = client
        .get_schedule()
        .name("crud-s1")
        .send()
        .await
        .expect_err("schedule should be gone");
    assert!(format!("{err:?}").contains("ResourceNotFound"));
}

#[tokio::test]
async fn scheduler_update_is_idempotent_upsert() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;

    client
        .create_schedule()
        .name("up-s1")
        .schedule_expression("rate(1 minute)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();

    let new_target = Target::builder()
        .arn("arn:aws:sqs:us-east-1:000000000000:updated-dest")
        .role_arn("arn:aws:iam::000000000000:role/scheduler")
        .input("{\"v\":2}")
        .build()
        .unwrap();

    client
        .update_schedule()
        .name("up-s1")
        .schedule_expression("rate(10 minutes)")
        .flexible_time_window(off_window())
        .target(new_target)
        .state(ScheduleState::Disabled)
        .send()
        .await
        .expect("update_schedule");

    let got = client.get_schedule().name("up-s1").send().await.unwrap();
    assert_eq!(got.schedule_expression().unwrap(), "rate(10 minutes)");
    assert_eq!(got.state().unwrap(), &ScheduleState::Disabled);
    assert_eq!(got.target().unwrap().input().unwrap(), "{\"v\":2}");
}

#[tokio::test]
async fn scheduler_at_one_shot_action_after_completion_persists() {
    // Round-trip the one-shot configuration; firing semantics land in Batch 2.
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;

    client
        .create_schedule()
        .name("once")
        .schedule_expression("at(2099-01-01T12:00:00)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .action_after_completion(ActionAfterCompletion::Delete)
        .send()
        .await
        .expect("create at-schedule");

    let got = client.get_schedule().name("once").send().await.unwrap();
    assert_eq!(
        got.action_after_completion().unwrap(),
        &ActionAfterCompletion::Delete
    );
    assert_eq!(
        got.schedule_expression().unwrap(),
        "at(2099-01-01T12:00:00)"
    );
}

#[tokio::test]
async fn scheduler_list_schedules_filters() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;

    client
        .create_schedule_group()
        .name("groupX")
        .send()
        .await
        .unwrap();

    for (name, group) in [
        ("alpha-1", "default"),
        ("alpha-2", "groupX"),
        ("beta-1", "groupX"),
    ] {
        client
            .create_schedule()
            .name(name)
            .group_name(group)
            .schedule_expression("rate(1 hour)")
            .flexible_time_window(off_window())
            .target(sqs_target())
            .send()
            .await
            .unwrap();
    }

    let resp = client
        .list_schedules()
        .group_name("groupX")
        .name_prefix("alpha")
        .send()
        .await
        .unwrap();
    let names: Vec<&str> = resp.schedules().iter().map(|s| s.name().unwrap()).collect();
    assert_eq!(names, ["alpha-2"]);
}

#[tokio::test]
async fn scheduler_group_lifecycle() {
    let server = TestServer::start().await;
    let client = server.scheduler_client().await;

    let created = client
        .create_schedule_group()
        .name("life-grp")
        .send()
        .await
        .unwrap();
    assert!(created
        .schedule_group_arn()
        .contains("schedule-group/life-grp"));

    let got = client
        .get_schedule_group()
        .name("life-grp")
        .send()
        .await
        .unwrap();
    assert_eq!(got.name().unwrap(), "life-grp");

    client
        .delete_schedule_group()
        .name("life-grp")
        .send()
        .await
        .unwrap();

    let err = client
        .get_schedule_group()
        .name("life-grp")
        .send()
        .await
        .expect_err("group should be gone");
    assert!(format!("{err:?}").contains("ResourceNotFound"));
}
