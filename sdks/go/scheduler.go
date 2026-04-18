package fakecloud

import "context"

// SchedulerClient provides access to EventBridge Scheduler
// introspection endpoints. Exposes the two hooks integration tests
// need: enumerate schedules registered on the server, and trigger a
// specific schedule to fire on demand.
type SchedulerClient struct {
	fc *FakeCloud
}

// GetSchedules returns every schedule the server knows about, across
// every account. Order is stable: by account, then group, then name.
func (c *SchedulerClient) GetSchedules(ctx context.Context) (*SchedulerSchedulesResponse, error) {
	var out SchedulerSchedulesResponse
	if err := c.fc.doGet(ctx, "/_fakecloud/scheduler/schedules", &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// FireSchedule triggers the named schedule immediately, bypassing the
// wall-clock tick. Applies the same post-fire handling as the normal
// loop (last_fired update, ActionAfterCompletion=DELETE cleanup).
func (c *SchedulerClient) FireSchedule(ctx context.Context, group, name string) (*FireScheduleResponse, error) {
	var out FireScheduleResponse
	if err := c.fc.doPost(ctx, "/_fakecloud/scheduler/fire/"+group+"/"+name, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}
