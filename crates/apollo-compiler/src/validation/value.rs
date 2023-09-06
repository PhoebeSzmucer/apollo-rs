use crate::{
    ast,
    diagnostics::{ApolloDiagnostic, DiagnosticData, Label},
    hir::{self, TypeDefinition, Value},
    schema,
    validation::ValidationDatabase,
    Arc, Node,
};
use apollo_parser::cst::{self, CstNode};

macro_rules! unsupported_type {
    ($db: expr, $value: expr, $type_def: expr) => {{
        ApolloDiagnostic::new(
            $db,
            $value.loc().into(),
            DiagnosticData::UnsupportedValueType {
                value: $value.kind().into(),
                ty: $type_def.name().into(),
            },
        )
        .labels([
            Label::new(
                $type_def.loc().unwrap(),
                format!("field declared here as {} type", $type_def.name()),
            ),
            Label::new(
                $value.loc(),
                format!("argument declared here is of {} type", $value.kind()),
            ),
        ])
    }};
}

fn unsupported_type(
    db: &dyn ValidationDatabase,
    value: &Node<ast::Value>,
    declared_type: &ast::Type,
) -> ApolloDiagnostic {
    let type_location = super::lookup_cst_location(
        db.upcast(),
        *declared_type.inner_named_type().location().unwrap(),
        |cst: cst::Type| Some(cst.syntax().text_range()),
    );

    ApolloDiagnostic::new(
        db,
        (*value.location().unwrap()).into(),
        DiagnosticData::UnsupportedValueType {
            value: value.kind().into(),
            ty: declared_type.to_string(),
        },
    )
    .labels([
        Label::new(
            type_location.unwrap(),
            format!("field declared here as {} type", declared_type),
        ),
        Label::new(
            *value.location().unwrap(),
            format!("argument declared here is of {} type", value.kind()),
        ),
    ])
}

//for bigint
/*
*/
pub fn validate_values2(
    db: &dyn ValidationDatabase,
    ty: &ast::Type,
    (arg_name, arg_value): (&ast::Name, Node<ast::Value>),
    var_defs: &[Node<ast::VariableDefinition>],
) -> Vec<ApolloDiagnostic> {
    let mut diagnostics = vec![];
    value_of_correct_type2(db, ty, &arg_value, var_defs, &mut diagnostics);
    diagnostics
}

pub fn value_of_correct_type2(
    db: &dyn ValidationDatabase,
    ty: &ast::Type,
    arg_value: &Node<ast::Value>,
    var_defs: &[Node<ast::VariableDefinition>],
    diagnostics: &mut Vec<ApolloDiagnostic>,
) {
    let schema = db.schema();
    let Some(type_definition) = schema.types.get(ty.inner_named_type()) else {
        return;
    };

    match &**arg_value {
        // When expected as an input type, only integer input values are
        // accepted. All other input values, including strings with numeric
        // content, must raise a request error indicating an incorrect
        // type. If the integer input value represents a value less than
        // -231 or greater than or equal to 231, a request error should be
        // raised.
        // When expected as an input type, any string (such as "4") or
        // integer (such as 4 or -4) input value should be coerced to ID
        ast::Value::Int(int) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) => {
                if scalar.is_built_in()
                    && matches!(ty.inner_named_type().as_str(), "String" | "Boolean")
                {
                    diagnostics.push(unsupported_type(db, &arg_value, ty));
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        ast::Value::BigInt(int) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) => {
                if scalar.is_built_in()
                    && matches!(ty.inner_named_type().as_str(), "Int" | "Float" | "ID")
                {
                    if int.parse::<i32>().is_err() {
                        diagnostics.push(
                            ApolloDiagnostic::new(
                                db,
                                (*arg_value.location().unwrap()).into(),
                                DiagnosticData::IntCoercionError {
                                    value: int.to_string(),
                                },
                            )
                            .label(Label::new(
                                *arg_value.location().unwrap(),
                                "cannot be coerced to an 32-bit integer",
                            )),
                        )
                    }
                } else if scalar.is_built_in()
                    && matches!(ty.inner_named_type().as_str(), "String" | "Boolean")
                {
                    diagnostics.push(unsupported_type(db, &arg_value, ty));
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        // When expected as an input type, both integer and float input
        // values are accepted. All other input values, including strings
        // with numeric content, must raise a request error indicating an
        // incorrect type.
        ast::Value::Float(_) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) => {
                if scalar.is_built_in() && ty.inner_named_type() != "Float" {
                    diagnostics.push(unsupported_type(db, &arg_value, ty));
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        // When expected as an input type, only valid Unicode string input
        // values are accepted. All other input values must raise a request
        // error indicating an incorrect type.
        // When expected as an input type, any string (such as "4") or
        // integer (such as 4 or -4) input value should be coerced to ID
        ast::Value::String(_) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) => {
                // specifically return diagnostics for ints, floats, and
                // booleans.
                // string, ids and custom scalars are ok, and
                // don't need a diagnostic.
                if scalar.is_built_in()
                    && !matches!(ty.inner_named_type().as_str(), "String" | "ID")
                {
                    diagnostics.push(unsupported_type(db, &arg_value, ty));
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        // When expected as an input type, only boolean input values are
        // accepted. All other input values must raise a request error
        // indicating an incorrect type.
        ast::Value::Boolean(_) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) => {
                if scalar.is_built_in() && ty.inner_named_type().as_str() == "Boolean" {
                    diagnostics.push(unsupported_type(db, &arg_value, ty));
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        ast::Value::Null => {
            if !matches!(
                type_definition,
                schema::ExtendedType::Enum(_) | schema::ExtendedType::Scalar(_)
            ) {
                diagnostics.push(unsupported_type(db, &arg_value, ty));
            }
        }
        ast::Value::Variable(var_name) => match &type_definition {
            schema::ExtendedType::Scalar(_)
            | schema::ExtendedType::Enum(_)
            | schema::ExtendedType::InputObject(_) => {
                let var_def = var_defs.iter().find(|v| v.name == *var_name);
                if let Some(var_def) = var_def {
                    // we don't have the actual variable values here, so just
                    // compare if two Types are the same
                    // TODO(@goto-bus-stop) This should use the is_assignable_to check
                    if var_def.ty.inner_named_type() != ty.inner_named_type() {
                        diagnostics.push(unsupported_type(db, &arg_value, ty));
                    } else if let Some(default_value) = &var_def.default_value {
                        if var_def.ty.is_non_null() && default_value.is_null() {
                            diagnostics.push(unsupported_type(db, &default_value, &var_def.ty))
                        } else {
                            value_of_correct_type2(
                                db,
                                &var_def.ty,
                                &default_value,
                                var_defs,
                                diagnostics,
                            )
                        }
                    }
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        // GraphQL has a constant literal to represent enum input values.
        // GraphQL string literals must not be accepted as an enum input and
        // instead raise a request error.
        ast::Value::Enum(value) => match &type_definition {
            schema::ExtendedType::Enum(enum_) => {
                if !enum_.values.contains_key(value) {
                    diagnostics.push(
                        ApolloDiagnostic::new(
                            db,
                            (*value.location().unwrap()).into(),
                            DiagnosticData::UndefinedValue {
                                value: value.to_string(),
                                definition: ty.inner_named_type().to_string(),
                            },
                        )
                        .label(Label::new(
                            *arg_value.location().unwrap(),
                            format!("does not exist on `{}` type", ty.inner_named_type()),
                        )),
                    );
                }
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        // When expected as an input, list values are accepted only when
        // each item in the list can be accepted by the list’s item type.
        //
        // If the value passed as an input to a list type is not a list and
        // not the null value, then the result of input coercion is a list
        // of size one, where the single item value is the result of input
        // coercion for the list’s item type on the provided value (note
        // this may apply recursively for nested lists).
        ast::Value::List(li) => match &type_definition {
            schema::ExtendedType::Scalar(_)
            | schema::ExtendedType::Enum(_)
            | schema::ExtendedType::InputObject(_) => li
                .iter()
                .for_each(|v| value_of_correct_type2(db, ty, v, var_defs, diagnostics)),
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
        ast::Value::Object(obj) => match &type_definition {
            schema::ExtendedType::Scalar(scalar) if !scalar.is_built_in() => (),
            schema::ExtendedType::InputObject(input_obj) => {
                let undefined_field = obj
                    .iter()
                    .find(|(name, ..)| !input_obj.fields.contains_key(name));

                // Add a diagnostic if a value does not exist on the input
                // object type
                if let Some((name, value)) = undefined_field {
                    diagnostics.push(
                        ApolloDiagnostic::new(
                            db,
                            (*value.location().unwrap()).into(),
                            DiagnosticData::UndefinedValue {
                                value: name.to_string(),
                                definition: ty.inner_named_type().to_string(),
                            },
                        )
                        .label(Label::new(
                            *value.location().unwrap(),
                            format!("does not exist on `{}` type", ty.inner_named_type()),
                        )),
                    );
                }

                input_obj.fields.iter().for_each(|(input_name, f)| {
                    let ty = &f.ty;
                    let is_missing = !obj.iter().any(|(value_name, ..)| input_name == value_name);
                    let is_null = obj
                        .iter()
                        .any(|(name, value)| input_name == name && value.is_null());

                    // If the input object field type is non_null, and no
                    // default value is provided, or if the value provided
                    // is null or missing entirely, an error should be
                    // raised.
                    if (ty.is_non_null() && f.default_value.is_none()) && (is_missing || is_null) {
                        let mut diagnostic = ApolloDiagnostic::new(
                            db,
                            (*arg_value.location().unwrap()).into(),
                            DiagnosticData::RequiredArgument {
                                name: input_name.to_string(),
                            },
                        );
                        diagnostic = diagnostic.label(Label::new(
                            *arg_value.location().unwrap(),
                            format!("missing value for argument `{input_name}`"),
                        ));
                        if let Some(&loc) = f.location() {
                            diagnostic = diagnostic.label(Label::new(loc, "argument defined here"));
                        }

                        diagnostics.push(diagnostic)
                    }

                    let used_val = obj.iter().find(|(obj_name, ..)| obj_name == input_name);

                    if let Some((_, v)) = used_val {
                        value_of_correct_type2(db, ty, v, var_defs, diagnostics);
                    }
                })
            }
            _ => diagnostics.push(unsupported_type(db, &arg_value, ty)),
        },
    }
}

pub fn validate_values(
    db: &dyn ValidationDatabase,
    ty: &hir::Type,
    arg: &hir::Argument,
    var_defs: Arc<Vec<hir::VariableDefinition>>,
) -> Result<(), Vec<ApolloDiagnostic>> {
    let mut diagnostics = Vec::new();

    value_of_correct_type(db, ty, arg.value(), var_defs, &mut diagnostics);

    match diagnostics.len() {
        0 => Ok(()),
        _ => Err(diagnostics),
    }
}

pub fn value_of_correct_type(
    db: &dyn ValidationDatabase,
    ty: &hir::Type,
    val: &Value,
    var_defs: Arc<Vec<hir::VariableDefinition>>,
    diagnostics: &mut Vec<ApolloDiagnostic>,
) {
    let type_def = ty.type_def(db.upcast());
    if let Some(type_def) = type_def {
        match val {
            // When expected as an input type, only integer input values are
            // accepted. All other input values, including strings with numeric
            // content, must raise a request error indicating an incorrect
            // type. If the integer input value represents a value less than
            // -231 or greater than or equal to 231, a request error should be
            // raised.
            // When expected as an input type, any string (such as "4") or
            // integer (such as 4 or -4) input value should be coerced to ID
            Value::Int { value: int, .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(scalar) => {
                    if scalar.is_int() || scalar.is_float() || scalar.is_id() {
                        if int.to_i32_checked().is_none() {
                            diagnostics.push(
                                ApolloDiagnostic::new(
                                    db,
                                    val.loc().into(),
                                    DiagnosticData::IntCoercionError {
                                        value: int.get().to_string(),
                                    },
                                )
                                .label(Label::new(
                                    val.loc(),
                                    "cannot be coerced to an 32-bit integer",
                                )),
                            )
                        }
                    } else if scalar.is_string() || scalar.is_boolean() {
                        diagnostics.push(unsupported_type!(db, val, ty));
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            // When expected as an input type, both integer and float input
            // values are accepted. All other input values, including strings
            // with numeric content, must raise a request error indicating an
            // incorrect type.
            Value::Float { .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(scalar) => {
                    if !scalar.is_float() && !scalar.is_custom() {
                        diagnostics.push(unsupported_type!(db, val, ty));
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            // When expected as an input type, only valid Unicode string input
            // values are accepted. All other input values must raise a request
            // error indicating an incorrect type.
            // When expected as an input type, any string (such as "4") or
            // integer (such as 4 or -4) input value should be coerced to ID
            Value::String { .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(scalar) => {
                    // specifically return diagnostics for ints, floats, and
                    // booleans.
                    // string, ids and custom scalars are ok, and
                    // don't need a diagnostic.
                    if !scalar.is_string() && !scalar.is_id() && !scalar.is_custom() {
                        diagnostics.push(unsupported_type!(db, val, ty));
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            // When expected as an input type, only boolean input values are
            // accepted. All other input values must raise a request error
            // indicating an incorrect type.
            Value::Boolean { .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(scalar) => {
                    if !scalar.is_boolean() && !scalar.is_custom() {
                        diagnostics.push(unsupported_type!(db, val, ty));
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            Value::Null { .. } => {
                if !type_def.is_enum_type_definition() && !type_def.is_scalar_type_definition() {
                    diagnostics.push(unsupported_type!(db, val, ty));
                }
            }
            Value::Variable(ref var) => match &type_def {
                TypeDefinition::ScalarTypeDefinition(_)
                | TypeDefinition::EnumTypeDefinition(_)
                | TypeDefinition::InputObjectTypeDefinition(_) => {
                    let var_def = var_defs.iter().find(|v| v.name() == var.name());
                    if let Some(var_def) = var_def {
                        // we don't have the actual variable values here, so just
                        // compare if two Types are the same
                        if var_def.ty().name() != type_def.name() {
                            diagnostics.push(unsupported_type!(db, val.clone(), ty));
                        } else if let Some(default_value) = var_def.default_value() {
                            if var_def.ty().is_non_null() && default_value.is_null() {
                                diagnostics.push(unsupported_type!(db, default_value, var_def.ty()))
                            } else {
                                value_of_correct_type(
                                    db,
                                    var_def.ty(),
                                    default_value,
                                    var_defs.clone(),
                                    diagnostics,
                                )
                            }
                        }
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            // GraphQL has a constant literal to represent enum input values.
            // GraphQL string literals must not be accepted as an enum input and
            // instead raise a request error.
            Value::Enum { ref value, loc } => match &type_def {
                TypeDefinition::EnumTypeDefinition(enum_) => {
                    let enum_val = enum_.values().find(|v| v.enum_value() == value.src());
                    if enum_val.is_none() {
                        diagnostics.push(
                            ApolloDiagnostic::new(
                                db,
                                (*loc).into(),
                                DiagnosticData::UndefinedValue {
                                    value: value.src().into(),
                                    definition: type_def.name().into(),
                                },
                            )
                            .label(Label::new(
                                val.loc(),
                                format!("does not exist on `{}` type", type_def.name()),
                            )),
                        );
                    }
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            // When expected as an input, list values are accepted only when
            // each item in the list can be accepted by the list’s item type.
            //
            // If the value passed as an input to a list type is not a list and
            // not the null value, then the result of input coercion is a list
            // of size one, where the single item value is the result of input
            // coercion for the list’s item type on the provided value (note
            // this may apply recursively for nested lists).
            Value::List { value: ref li, .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(_)
                | TypeDefinition::EnumTypeDefinition(_)
                | TypeDefinition::InputObjectTypeDefinition(_) => li
                    .iter()
                    .for_each(|v| value_of_correct_type(db, ty, v, var_defs.clone(), diagnostics)),
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
            Value::Object { value: ref obj, .. } => match &type_def {
                TypeDefinition::ScalarTypeDefinition(scalar) if scalar.is_custom() => (),
                TypeDefinition::InputObjectTypeDefinition(input_obj) => {
                    let undefined_field = obj
                        .iter()
                        .find(|(name, ..)| !input_obj.fields().any(|f| f.name() == name.src()));

                    // Add a diagnostic if a value does not exist on the input
                    // object type
                    if let Some((name, value)) = undefined_field {
                        diagnostics.push(
                            ApolloDiagnostic::new(
                                db,
                                value.loc().into(),
                                DiagnosticData::UndefinedValue {
                                    value: name.src().into(),
                                    definition: type_def.name().into(),
                                },
                            )
                            .label(Label::new(
                                value.loc(),
                                format!("does not exist on `{}` type", type_def.name()),
                            )),
                        );
                    }

                    input_obj.fields().for_each(|f| {
                        let ty = f.ty();
                        let is_missing = !obj.iter().any(|(name, ..)| f.name() == name.src());
                        let is_null = obj
                            .iter()
                            .any(|(name, value)| f.name() == name.src() && value.is_null());

                        // If the input object field type is non_null, and no
                        // default value is provided, or if the value provided
                        // is null or missing entirely, an error should be
                        // raised.
                        if (ty.is_non_null() && f.default_value().is_none())
                            && (is_missing || is_null)
                        {
                            let mut diagnostic = ApolloDiagnostic::new(
                                db,
                                val.loc().into(),
                                DiagnosticData::RequiredArgument {
                                    name: f.name().into(),
                                },
                            );
                            diagnostic = diagnostic.label(Label::new(
                                val.loc(),
                                format!("missing value for argument `{}`", f.name()),
                            ));
                            if let Some(loc) = ty.loc() {
                                diagnostic =
                                    diagnostic.label(Label::new(loc, "argument defined here"));
                            }

                            diagnostics.push(diagnostic)
                        }

                        let used_val = obj.iter().find(|(name, ..)| name.src() == f.name());

                        if let Some((_, v)) = used_val {
                            value_of_correct_type(db, ty, v, var_defs.clone(), diagnostics);
                        }
                    })
                }
                _ => diagnostics.push(unsupported_type!(db, val, ty)),
            },
        };
    }
}
