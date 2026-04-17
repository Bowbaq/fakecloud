//! AWS EventBridge Scheduler (`scheduler.amazonaws.com`).
//!
//! Distinct from EventBridge Rules (`events.amazonaws.com`): Scheduler
//! is a standalone service with its own SDK, data model (Schedule,
//! ScheduleGroup, FlexibleTimeWindow, DeadLetterConfig,
//! ActionAfterCompletion), and REST-JSON protocol.

pub mod service;
pub mod state;
