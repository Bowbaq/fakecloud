//! Strategy 1: Constraint-aware boundary value generation.
//!
//! For each constrained field in the input, generate variants that exercise
//! min, max, and mid-range values.

use serde_json::Value;
use std::collections::HashMap;

use super::{build_required_input, Expectation, Strategy, TestVariant};
use crate::smithy::{self, ServiceModel, ShapeType};

pub fn generate(
    model: &ServiceModel,
    input_shape_id: &str,
    overrides: &HashMap<String, Value>,
) -> Vec<TestVariant> {
    let mut variants = Vec::new();

    let members = super::get_members(model, input_shape_id);

    for member in members {
        let target_shape = model
            .shapes
            .get(&member.target)
            .or_else(|| smithy::prelude_shape_type(&member.target).map(|_| &*PLACEHOLDER_SHAPE));

        let shape = match target_shape {
            Some(s) => s,
            None => continue,
        };

        // Get constraints from the shape's traits AND the member's own traits
        let traits = &shape.traits;

        let has_length = traits.length_min.is_some() || traits.length_max.is_some();
        let has_range = traits.range_min.is_some() || traits.range_max.is_some();

        if !has_length && !has_range {
            continue;
        }

        // String length boundaries
        if has_length {
            if let ShapeType::String { .. } = &shape.shape_type {
                if let Some(min) = traits.length_min {
                    let min = min.max(1) as usize;
                    let val = "a".repeat(min);
                    let mut input = build_required_input(model, input_shape_id, overrides);
                    if let Value::Object(ref mut obj) = input {
                        obj.insert(member.name.clone(), Value::String(val));
                    }
                    variants.push(TestVariant {
                        name: format!("boundary_len_min_{}_{}", member.name, min),
                        strategy: Strategy::Boundary,
                        input,
                        expectation: Expectation::Success,
                    });
                }
                if let Some(max) = traits.length_max {
                    let max = max as usize;
                    if max <= 10000 {
                        // Don't generate enormous strings
                        let val = "a".repeat(max);
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::String(val));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_len_max_{}_{}", member.name, max),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                }
                if let (Some(min), Some(max)) = (traits.length_min, traits.length_max) {
                    let mid = ((min + max) / 2) as usize;
                    if mid > 0 && mid < 10000 {
                        let val = "a".repeat(mid);
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::String(val));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_len_mid_{}_{}", member.name, mid),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                }
            }

            // List/Map length boundaries
            if let ShapeType::List { member_target } = &shape.shape_type {
                if let Some(min) = traits.length_min {
                    let min = min as usize;
                    if min > 0 && min <= 100 {
                        let items: Vec<Value> = (0..min)
                            .map(|_| super::default_value_for_shape(model, member_target, 0))
                            .collect();
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::Array(items));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_list_min_{}_{}", member.name, min),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                }
            }
        }

        // Numeric range boundaries
        if has_range {
            match &shape.shape_type {
                ShapeType::Integer | ShapeType::Long => {
                    if let Some(min) = traits.range_min {
                        let val = min as i64;
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::Number(val.into()));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_range_min_{}_{}", member.name, val),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                    if let Some(max) = traits.range_max {
                        let val = max as i64;
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::Number(val.into()));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_range_max_{}_{}", member.name, val),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                    if let (Some(min), Some(max)) = (traits.range_min, traits.range_max) {
                        let mid = ((min + max) / 2.0) as i64;
                        let mut input = build_required_input(model, input_shape_id, overrides);
                        if let Value::Object(ref mut obj) = input {
                            obj.insert(member.name.clone(), Value::Number(mid.into()));
                        }
                        variants.push(TestVariant {
                            name: format!("boundary_range_mid_{}_{}", member.name, mid),
                            strategy: Strategy::Boundary,
                            input,
                            expectation: Expectation::Success,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    variants
}

// Placeholder for prelude shapes that have no traits
use crate::smithy::Shape;
use std::sync::LazyLock;

static PLACEHOLDER_SHAPE: LazyLock<Shape> = LazyLock::new(|| Shape {
    shape_id: String::new(),
    shape_type: ShapeType::String { enum_values: None },
    traits: crate::smithy::ShapeTraits::default(),
});
