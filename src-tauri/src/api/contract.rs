//! The typed contract between the Rust commands and the TypeScript client.
//!
//! `schema()` produces one JSON schema with every command's input and output; the npm script
//! `gen:api` turns it into packages/shared/src/generated/api.ts. A test fails when the committed
//! schema is out of date (run `UPDATE_CONTRACT=1 cargo test contract` to refresh it).

use schemars::gen::{SchemaGenerator, SchemaSettings};
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde_json::{json, Map, Value};

pub struct Spec {
    name: &'static str,
    needs_session: bool,
    input: Schema,
    output: Schema,
}

impl Spec {
    pub fn new<I: JsonSchema, O: JsonSchema>(
        gen: &mut SchemaGenerator,
        name: &'static str,
        needs_session: bool,
    ) -> Self {
        Self {
            name,
            needs_session,
            input: gen.subschema_for::<I>(),
            output: gen.subschema_for::<O>(),
        }
    }
}

pub fn schema() -> Value {
    let mut gen = SchemaSettings::draft07().into_generator();
    let mut specs = super::specs(&mut gen);
    specs.sort_by_key(|s| s.name);
    let error = gen.subschema_for::<crate::error::AppError>();
    let approval = gen.subschema_for::<crate::services::auth::ApprovalInput>();

    let mut commands = Map::new();
    let mut public = Vec::new();
    for spec in &specs {
        if !spec.needs_session {
            public.push(spec.name);
        }
        commands.insert(
            spec.name.to_string(),
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["input", "output"],
                "properties": { "input": spec.input, "output": spec.output },
            }),
        );
    }
    let names: Vec<&str> = specs.iter().map(|s| s.name).collect();
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "ApiContract",
        "description": "Generated from src-tauri/src/api (do not edit). Every command takes { token, input, approval? }.",
        "type": "object",
        "additionalProperties": false,
        "required": ["approval", "commands", "error", "publicCommands"],
        "properties": {
            "approval": approval,
            "commands": { "type": "object", "additionalProperties": false, "required": names, "properties": commands },
            "error": error,
            "publicCommands": { "type": "array", "items": { "type": "string", "enum": public } },
        },
        "definitions": gen.definitions(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../packages/shared/src/generated/api-schema.json"
    );

    #[test]
    fn contract_schema_is_up_to_date() {
        let fresh = format!("{}\n", serde_json::to_string_pretty(&schema()).unwrap());
        if std::env::var_os("UPDATE_CONTRACT").is_some() {
            std::fs::create_dir_all(std::path::Path::new(PATH).parent().unwrap()).unwrap();
            std::fs::write(PATH, &fresh).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(PATH)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == fresh,
            "the API contract changed: run `UPDATE_CONTRACT=1 cargo test contract` and `npm run gen:api`, then commit both files"
        );
    }

    #[test]
    fn every_command_is_registered_in_the_handler() {
        let names: std::collections::BTreeSet<&str> = super::super::NAMES.iter().copied().collect();
        let mut gen = SchemaSettings::draft07().into_generator();
        let specs: std::collections::BTreeSet<&str> = super::super::specs(&mut gen)
            .iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, specs);
        assert_eq!(
            names.len(),
            super::super::NAMES.len(),
            "duplicate command names"
        );
    }
}
