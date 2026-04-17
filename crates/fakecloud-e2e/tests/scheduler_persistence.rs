//! Persistence round-trips for EventBridge Scheduler.

mod helpers;

use aws_sdk_scheduler::types::{
    ActionAfterCompletion, FlexibleTimeWindow, FlexibleTimeWindowMode, ScheduleState, Target,
};
use helpers::TestServer;

fn off_window() -> FlexibleTimeWindow {
    FlexibleTimeWindow::builder()
        .mode(FlexibleTimeWindowMode::Off)
        .build()
        .unwrap()
}

fn sqs_target() -> Target {
    Target::builder()
        .arn("arn:aws:sqs:us-east-1:000000000000:dest")
        .role_arn("arn:aws:iam::000000000000:role/s")
        .input("{\"v\":1}")
        .build()
        .unwrap()
}

#[tokio::test]
async fn schedule_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.scheduler_client().await;

    client
        .create_schedule_group()
        .name("customgrp")
        .send()
        .await
        .unwrap();
    client
        .create_schedule()
        .name("nightly")
        .group_name("customgrp")
        .schedule_expression("rate(1 day)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .state(ScheduleState::Disabled)
        .action_after_completion(ActionAfterCompletion::None)
        .description("backup job")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let client = server.scheduler_client().await;

    let got = client
        .get_schedule()
        .name("nightly")
        .group_name("customgrp")
        .send()
        .await
        .expect("schedule should survive restart");
    assert_eq!(got.schedule_expression().unwrap(), "rate(1 day)");
    assert_eq!(got.state().unwrap(), &ScheduleState::Disabled);
    assert_eq!(got.description().unwrap(), "backup job");
    assert_eq!(
        got.target().unwrap().arn(),
        "arn:aws:sqs:us-east-1:000000000000:dest"
    );

    let group = client
        .get_schedule_group()
        .name("customgrp")
        .send()
        .await
        .expect("group should survive restart");
    assert_eq!(group.name().unwrap(), "customgrp");
}

#[tokio::test]
async fn deleted_schedule_stays_deleted_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.scheduler_client().await;

    client
        .create_schedule()
        .name("temp")
        .schedule_expression("rate(1 hour)")
        .flexible_time_window(off_window())
        .target(sqs_target())
        .send()
        .await
        .unwrap();
    client.delete_schedule().name("temp").send().await.unwrap();

    server.restart().await;
    let client = server.scheduler_client().await;
    let err = client
        .get_schedule()
        .name("temp")
        .send()
        .await
        .expect_err("deleted schedule must not resurrect");
    assert!(format!("{err:?}").contains("ResourceNotFound"));
}
