package fakecloud

import (
	"context"
	"fmt"
)

// BedrockClient provides access to Bedrock introspection endpoints.
type BedrockClient struct {
	fc *FakeCloud
}

// GetInvocations lists all recorded Bedrock model invocations.
func (c *BedrockClient) GetInvocations(ctx context.Context) (*BedrockInvocationsResponse, error) {
	var out BedrockInvocationsResponse
	if err := c.fc.doGet(ctx, "/_fakecloud/bedrock/invocations", &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// SetModelResponse configures the canned response for a Bedrock model.
func (c *BedrockClient) SetModelResponse(ctx context.Context, modelID string, response string) (*BedrockModelResponseConfig, error) {
	var out BedrockModelResponseConfig
	if err := c.fc.doPostText(ctx, fmt.Sprintf("/_fakecloud/bedrock/models/%s/response", modelID), response, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// SetResponseRules replaces the prompt-conditional response rules for a model.
func (c *BedrockClient) SetResponseRules(ctx context.Context, modelID string, rules []BedrockResponseRule) (*BedrockModelResponseConfig, error) {
	var out BedrockModelResponseConfig
	body := struct {
		Rules []BedrockResponseRule `json:"rules"`
	}{Rules: rules}
	if err := c.fc.doPost(ctx, fmt.Sprintf("/_fakecloud/bedrock/models/%s/responses", modelID), body, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// ClearResponseRules clears all prompt-conditional response rules for a model.
func (c *BedrockClient) ClearResponseRules(ctx context.Context, modelID string) (*BedrockModelResponseConfig, error) {
	var out BedrockModelResponseConfig
	if err := c.fc.doDelete(ctx, fmt.Sprintf("/_fakecloud/bedrock/models/%s/responses", modelID), &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// QueueFault queues a fault rule that will cause the next matching Bedrock runtime call(s) to fail.
func (c *BedrockClient) QueueFault(ctx context.Context, rule BedrockFaultRule) (*BedrockStatusResponse, error) {
	var out BedrockStatusResponse
	if err := c.fc.doPost(ctx, "/_fakecloud/bedrock/faults", rule, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// GetFaults lists currently queued fault rules.
func (c *BedrockClient) GetFaults(ctx context.Context) (*BedrockFaultsResponse, error) {
	var out BedrockFaultsResponse
	if err := c.fc.doGet(ctx, "/_fakecloud/bedrock/faults", &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// ClearFaults clears all queued fault rules.
func (c *BedrockClient) ClearFaults(ctx context.Context) (*BedrockStatusResponse, error) {
	var out BedrockStatusResponse
	if err := c.fc.doDelete(ctx, "/_fakecloud/bedrock/faults", &out); err != nil {
		return nil, err
	}
	return &out, nil
}
