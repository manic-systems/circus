//! OpenAPI 3.1 description of the public REST API.
//!
//! Hand-written rather than derived from route handlers. The trade-off:
//! schema drift is possible if a handler changes without touching this
//! file, so update it alongside any breaking change to a request or
//! response type. The dependency surface stays small in exchange.
//!
//! Coverage policy: every route registered under `/api/v1` has an entry,
//! plus the public LDAP login and channel manifest endpoints. The cache
//! NAR/narinfo endpoints are intentionally omitted because they speak
//! the Nix binary cache protocol, not JSON, and clients of that protocol
//! do not consult OpenAPI docs.

use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

#[allow(clippy::too_many_lines)]
fn document() -> Value {
  json!({
    "openapi": "3.1.0",
    "info": {
      "title":       "circus API",
      "version":     env!("CARGO_PKG_VERSION"),
      "description": "REST API for the circus continuous integration server.",
      "license":     { "name": "MPL-2.0" }
    },
    "servers": [
      { "url": "/api/v1", "description": "Versioned API root" }
    ],
    "components": {
      "securitySchemes": {
        "ApiKeyAuth": {
          "type": "apiKey",
          "in":   "header",
          "name": "X-API-Key"
        }
      },
      "schemas": {
        "Uuid":      { "type": "string", "format": "uuid" },
        "Timestamp": { "type": "string", "format": "date-time" },
        "Error": {
          "type": "object",
          "required": ["error"],
          "properties": { "error": { "type": "string" } }
        },
        "BuildStatus": {
          "type": "string",
          "enum": [
            "pending", "running", "succeeded", "failed",
            "cancelled", "aborted", "unsupported_system",
            "cached_failure"
          ]
        },
        "Build": {
          "type": "object",
          "required": ["id", "evaluation_id", "job_name", "status", "drv_path"],
          "properties": {
            "id":                { "$ref": "#/components/schemas/Uuid" },
            "evaluation_id":     { "$ref": "#/components/schemas/Uuid" },
            "job_name":          { "type": "string" },
            "system":            { "type": ["string", "null"] },
            "drv_path":          { "type": "string" },
            "outputs":           { "type": ["object", "null"] },
            "build_output_path": { "type": ["string", "null"] },
            "log_path":          { "type": ["string", "null"] },
            "status":            { "$ref": "#/components/schemas/BuildStatus" },
            "priority":          { "type": "integer" },
            "is_aggregate":      { "type": "boolean" },
            "retry_count":       { "type": "integer" },
            "max_retries":       { "type": "integer" },
            "builder_id":        { "type": ["string", "null"], "format": "uuid" },
            "keep":              { "type": "boolean" },
            "created_at":        { "$ref": "#/components/schemas/Timestamp" },
            "started_at":        { "type": ["string", "null"], "format": "date-time" },
            "completed_at":      { "type": ["string", "null"], "format": "date-time" }
          }
        },
        "BuildStep": {
          "type": "object",
          "required": ["id", "build_id", "step_number"],
          "properties": {
            "id":          { "$ref": "#/components/schemas/Uuid" },
            "build_id":    { "$ref": "#/components/schemas/Uuid" },
            "step_number": { "type": "integer" },
            "command":     { "type": "string" },
            "exit_code":   { "type": ["integer", "null"] },
            "stdout":      { "type": ["string", "null"] },
            "stderr":      { "type": ["string", "null"] },
            "started_at":  { "type": ["string", "null"], "format": "date-time" },
            "finished_at": { "type": ["string", "null"], "format": "date-time" }
          }
        },
        "BuildProduct": {
          "type": "object",
          "required": ["id", "build_id", "name", "path"],
          "properties": {
            "id":           { "$ref": "#/components/schemas/Uuid" },
            "build_id":     { "$ref": "#/components/schemas/Uuid" },
            "name":         { "type": "string" },
            "path":         { "type": "string" },
            "sha256_hash":  { "type": ["string", "null"] },
            "file_size":    { "type": ["integer", "null"], "format": "int64" },
            "content_type": { "type": ["string", "null"] },
            "is_directory": { "type": "boolean" },
            "gc_root_path": { "type": ["string", "null"] }
          }
        },
        "Project": {
          "type": "object",
          "required": ["id", "name", "repository_url"],
          "properties": {
            "id":             { "$ref": "#/components/schemas/Uuid" },
            "name":           { "type": "string" },
            "description":    { "type": ["string", "null"] },
            "repository_url": { "type": "string" },
            "enabled":        { "type": "boolean" },
            "created_at":     { "$ref": "#/components/schemas/Timestamp" }
          }
        },
        "Jobset": {
          "type": "object",
          "required": ["id", "project_id", "name", "nix_expression"],
          "properties": {
            "id":             { "$ref": "#/components/schemas/Uuid" },
            "project_id":     { "$ref": "#/components/schemas/Uuid" },
            "name":           { "type": "string" },
            "description":    { "type": ["string", "null"] },
            "nix_expression": { "type": "string" },
            "flake_mode":     { "type": "boolean" },
            "enabled":        { "type": "boolean" },
            "check_interval": { "type": "integer" },
            "state":          { "type": "string" }
          }
        },
        "JobsetInput": {
          "type": "object",
          "required": ["id", "jobset_id", "name", "input_type", "value"],
          "properties": {
            "id":         { "$ref": "#/components/schemas/Uuid" },
            "jobset_id":  { "$ref": "#/components/schemas/Uuid" },
            "name":       { "type": "string" },
            "input_type": { "type": "string" },
            "value":      { "type": "string" }
          }
        },
        "Evaluation": {
          "type": "object",
          "required": ["id", "jobset_id", "commit_hash", "status"],
          "properties": {
            "id":              { "$ref": "#/components/schemas/Uuid" },
            "jobset_id":       { "$ref": "#/components/schemas/Uuid" },
            "commit_hash":     { "type": "string" },
            "evaluation_time": { "$ref": "#/components/schemas/Timestamp" },
            "status":          { "type": "string" }
          }
        },
        "Channel": {
          "type": "object",
          "required": ["id", "project_id", "name", "jobset_id"],
          "properties": {
            "id":                    { "$ref": "#/components/schemas/Uuid" },
            "project_id":            { "$ref": "#/components/schemas/Uuid" },
            "name":                  { "type": "string" },
            "jobset_id":             { "$ref": "#/components/schemas/Uuid" },
            "current_evaluation_id": { "type": ["string", "null"], "format": "uuid" }
          }
        },
        "RemoteBuilder": {
          "type": "object",
          "required": ["id", "name", "ssh_uri", "systems", "max_jobs"],
          "properties": {
            "id":                  { "$ref": "#/components/schemas/Uuid" },
            "name":                { "type": "string" },
            "ssh_uri":             { "type": "string" },
            "ssh_key_file":        { "type": ["string", "null"] },
            "systems":             { "type": "array", "items": { "type": "string" } },
            "max_jobs":            { "type": "integer" },
            "speed_factor":        { "type": "integer" },
            "cpu_cores":           { "type": ["integer", "null"] },
            "supported_features":  { "type": "array", "items": { "type": "string" } },
            "mandatory_features":  { "type": "array", "items": { "type": "string" } },
            "enabled":             { "type": "boolean" }
          }
        },
        "User": {
          "type": "object",
          "required": ["id", "username", "user_type"],
          "properties": {
            "id":            { "$ref": "#/components/schemas/Uuid" },
            "username":      { "type": "string" },
            "email":         { "type": ["string", "null"], "format": "email" },
            "user_type":     {
              "type": "string",
              "enum": ["local", "github", "google", "ldap"]
            },
            "role":          { "type": "string" },
            "created_at":    { "$ref": "#/components/schemas/Timestamp" },
            "last_login_at": { "type": ["string", "null"], "format": "date-time" }
          }
        },
        "ApiKey": {
          "type": "object",
          "required": ["id", "name", "role"],
          "properties": {
            "id":           { "$ref": "#/components/schemas/Uuid" },
            "name":         { "type": "string" },
            "role":         { "type": "string" },
            "created_at":   { "$ref": "#/components/schemas/Timestamp" },
            "last_used_at": { "type": ["string", "null"], "format": "date-time" }
          }
        },
        "StarredJob": {
          "type": "object",
          "required": ["id", "user_id", "project_id", "jobset_id", "job_name"],
          "properties": {
            "id":         { "$ref": "#/components/schemas/Uuid" },
            "user_id":    { "$ref": "#/components/schemas/Uuid" },
            "project_id": { "$ref": "#/components/schemas/Uuid" },
            "jobset_id":  { "$ref": "#/components/schemas/Uuid" },
            "job_name":   { "type": "string" }
          }
        },
        "SystemStatus": {
          "type": "object",
          "properties": {
            "queue_depth":   { "type": "integer" },
            "active_builds": { "type": "integer" },
            "version":       { "type": "string" }
          }
        }
      }
    },
    "security": [{ "ApiKeyAuth": [] }],
    "paths": {
      "/projects": {
        "get":  { "summary": "List projects",
          "responses": { "200": { "description": "Array of projects",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Project" } }
            } } } } },
        "post": { "summary": "Create a project", "responses": { "200": { "description": "Created project" } } }
      },
      "/projects/probe": {
        "post": { "summary": "Probe a repository URL for jobset hints",
          "responses": { "200": { "description": "Probe result" } } }
      },
      "/projects/setup": {
        "post": { "summary": "Create project + initial jobset in one call",
          "responses": { "200": { "description": "Setup result" } } }
      },
      "/projects/{id}": {
        "get":    { "summary": "Get a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Project",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Project" } } } },
            "404": { "description": "Not found" } } },
        "put":    { "summary": "Update a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Updated" } } },
        "delete": { "summary": "Delete a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/projects/{id}/jobsets": {
        "get":  { "summary": "List jobsets for a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Jobset" } }
            } } } } },
        "post": { "summary": "Create a jobset in a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Created jobset" } } }
      },
      "/projects/{id}/builds": {
        "get": { "summary": "List builds for a project",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array of builds" } } }
      },
      "/projects/{id}/webhooks": {
        "get":  { "summary": "List configured webhooks",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array" } } },
        "post": { "summary": "Register a webhook",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Created" } } }
      },
      "/projects/{id}/webhooks/{webhook_id}": {
        "delete": { "summary": "Delete a webhook",
          "parameters": [
            { "name": "id",         "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "webhook_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/projects/{project_id}/jobsets/{id}": {
        "get":    { "summary": "Get a jobset",
          "parameters": [
            { "name": "project_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "id",         "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "200": { "description": "Jobset" } } },
        "put":    { "summary": "Update a jobset",
          "parameters": [
            { "name": "project_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "id",         "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "200": { "description": "Updated" } } },
        "delete": { "summary": "Delete a jobset",
          "parameters": [
            { "name": "project_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "id",         "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/projects/{project_id}/jobsets/{jobset_id}/inputs": {
        "get":  { "summary": "List jobset inputs",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/JobsetInput" } }
            } } } } },
        "post": { "summary": "Create a jobset input", "responses": { "200": { "description": "Created" } } }
      },
      "/projects/{project_id}/jobsets/{jobset_id}/inputs/{input_id}": {
        "delete": { "summary": "Delete a jobset input", "responses": { "204": { "description": "Deleted" } } }
      },
      "/evaluations": {
        "get": { "summary": "List evaluations",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Evaluation" } }
            } } } } }
      },
      "/evaluations/{id}": {
        "get": { "summary": "Get an evaluation",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Evaluation" } } }
      },
      "/evaluations/{id}/compare": {
        "get": { "summary": "Compare an evaluation against another",
          "parameters": [
            { "name": "id",    "in": "path",  "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "other", "in": "query", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "200": { "description": "Diff" } } }
      },
      "/evaluations/trigger": {
        "post": { "summary": "Trigger an evaluation",
          "responses": { "202": { "description": "Accepted" } } }
      },
      "/builds": {
        "get": { "summary": "List builds",
          "responses": { "200": { "description": "Array of builds",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Build" } }
            } } } } }
      },
      "/builds/stats":  { "get": { "summary": "Build statistics",      "responses": { "200": { "description": "Stats" } } } },
      "/builds/recent": { "get": { "summary": "Most recent builds",    "responses": { "200": { "description": "Array" } } } },
      "/builds/{id}": {
        "get": { "summary": "Get a build",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Build",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Build" } } } } } }
      },
      "/builds/{id}/cancel":  { "post": { "summary": "Cancel a build",
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
        "responses": { "200": { "description": "Cancelled" } } } },
      "/builds/{id}/restart": { "post": { "summary": "Restart a build",
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
        "responses": { "200": { "description": "Restarted" } } } },
      "/builds/{id}/bump":    { "post": { "summary": "Bump build priority",
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
        "responses": { "200": { "description": "Bumped" } } } },
      "/builds/{id}/keep/{value}": {
        "put": { "summary": "Pin or unpin a build from GC",
          "parameters": [
            { "name": "id",    "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "value", "in": "path", "required": true, "schema": { "type": "boolean" } }
          ],
          "responses": { "200": { "description": "Updated" } } }
      },
      "/builds/{id}/steps": {
        "get": { "summary": "List build steps",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/BuildStep" } }
            } } } } }
      },
      "/builds/{id}/products": {
        "get": { "summary": "List build products",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/BuildProduct" } }
            } } } } }
      },
      "/builds/{build_id}/products/{product_id}/download": {
        "get": { "summary": "Download a build product (binary stream)",
          "parameters": [
            { "name": "build_id",   "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "product_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "200": { "description": "Octet stream" } } }
      },
      "/builds/{id}/constituents": {
        "get": { "summary": "List constituents of an aggregate build",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": {
            "200": { "description": "Array of constituent builds",
              "content": { "application/json": {
                "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Build" } }
              } } },
            "422": { "description": "Build is not an aggregate",
              "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } }
          } }
      },
      "/builds/{id}/log":        { "get": { "summary": "Get build log (text)",
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
        "responses": { "200": { "description": "Plain text log" } } } },
      "/builds/{id}/log/stream": { "get": { "summary": "SSE-stream the live build log",
        "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
        "responses": { "200": { "description": "text/event-stream" } } } },
      "/channels": {
        "get":  { "summary": "List channels",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Channel" } }
            } } } } },
        "post": { "summary": "Create a channel", "responses": { "200": { "description": "Channel" } } }
      },
      "/channels/{id}": {
        "get":    { "summary": "Get a channel",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Channel" } } },
        "delete": { "summary": "Delete a channel",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/channels/{id}/nixexprs.tar.xz": {
        "get": { "summary": "Download channel nixexprs tarball",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "tar.xz stream" } } }
      },
      "/channels/{channel_id}/promote/{eval_id}": {
        "post": { "summary": "Promote an evaluation to a channel",
          "parameters": [
            { "name": "channel_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } },
            { "name": "eval_id",    "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }
          ],
          "responses": { "200": { "description": "Promoted" } } }
      },
      "/projects/{project_id}/channels": {
        "get": { "summary": "List channels for a project",
          "parameters": [{ "name": "project_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array" } } }
      },
      "/search": {
        "get": { "summary": "Advanced search",
          "parameters": [{ "name": "q", "in": "query", "schema": { "type": "string" } }],
          "responses": { "200": { "description": "Search results" } } }
      },
      "/search/quick": {
        "get": { "summary": "Quick search (autocomplete)",
          "parameters": [{ "name": "q", "in": "query", "schema": { "type": "string" } }],
          "responses": { "200": { "description": "Quick results" } } }
      },
      "/me": {
        "get": { "summary": "Get the current user",
          "responses": { "200": { "description": "User",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/User" } } } } } },
        "put": { "summary": "Update the current user",
          "responses": { "200": { "description": "Updated" } } }
      },
      "/me/password": {
        "post": { "summary": "Change the current user's password",
          "responses": { "204": { "description": "Changed" } } }
      },
      "/me/starred-jobs": {
        "get":  { "summary": "List starred jobs for the current user",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/StarredJob" } }
            } } } } },
        "post": { "summary": "Star a job",
          "responses": { "200": { "description": "Created" } } }
      },
      "/me/starred-jobs/{id}": {
        "delete": { "summary": "Unstar a job",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/users": {
        "get":  { "summary": "List users",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/User" } }
            } } } } },
        "post": { "summary": "Create a user", "responses": { "200": { "description": "Created" } } }
      },
      "/users/{id}": {
        "get":    { "summary": "Get a user",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "User" } } },
        "put":    { "summary": "Update a user",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Updated" } } },
        "delete": { "summary": "Delete a user",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/api-keys": {
        "get":  { "summary": "List API keys",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/ApiKey" } }
            } } } } },
        "post": { "summary": "Create an API key", "responses": { "200": { "description": "Created with one-time key value" } } }
      },
      "/api-keys/{id}": {
        "delete": { "summary": "Revoke an API key",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Revoked" } } }
      },
      "/admin/system": {
        "get": { "summary": "System status",
          "responses": { "200": { "description": "Status",
            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SystemStatus" } } } } } }
      },
      "/admin/builders": {
        "get":  { "summary": "List remote builders",
          "responses": { "200": { "description": "Array",
            "content": { "application/json": {
              "schema": { "type": "array", "items": { "$ref": "#/components/schemas/RemoteBuilder" } }
            } } } } },
        "post": { "summary": "Register a builder",
          "responses": { "200": { "description": "Builder" } } }
      },
      "/admin/builders/{id}": {
        "get":    { "summary": "Get a builder",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Builder" } } },
        "put":    { "summary": "Update a builder",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Updated" } } },
        "delete": { "summary": "Remove a builder",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/admin/builders/sessions": {
        "get": { "summary": "List all persistent builder agent sessions (connected + historical)",
          "responses": { "200": { "description": "Array of BuilderSession rows" } } }
      },
      "/admin/builders/sessions/connected": {
        "get": { "summary": "List currently-connected builder agents",
          "responses": { "200": { "description": "Array of BuilderSession rows where connected = TRUE" } } }
      },
      "/admin/builders/sessions/{machine_id}": {
        "get": { "summary": "Get a single builder agent session by its stable machine_id",
          "parameters": [{ "name": "machine_id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": {
            "200": { "description": "BuilderSession" },
            "404": { "description": "No session with that machine_id" }
          } }
      },
      "/admin/config": {
        "get": { "summary": "Read the server config file",
          "responses": { "200": { "description": "Raw config body",
            "content": { "text/plain": { "schema": { "type": "string" } } } } } },
        "put": { "summary": "Replace the server config file",
          "responses": { "200": { "description": "Updated" } } }
      },
      "/admin/audit-log": {
        "get": { "summary": "Paginated audit log (admin only)",
          "parameters": [
            { "name": "limit",  "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 500 } },
            { "name": "offset", "in": "query", "schema": { "type": "integer", "minimum": 0 } }
          ],
          "responses": { "200": { "description": "Audit entries page" } } }
      },
      "/admin/notification-tasks": {
        "get": { "summary": "List pending notification delivery tasks",
          "responses": { "200": { "description": "Array of task records" } } }
      },
      "/admin/notification-tasks/{id}/retry": {
        "post": { "summary": "Retry a notification delivery task",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "202": { "description": "Retry scheduled" } } }
      },
      "/news": {
        "get":  { "summary": "List news/announcement entries",
          "responses": { "200": { "description": "Array of news items" } } },
        "post": { "summary": "Create a news entry (admin only)",
          "responses": { "201": { "description": "Created" } } }
      },
      "/news/{id}": {
        "delete": { "summary": "Delete a news entry (admin only)",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "204": { "description": "Deleted" } } }
      },
      "/builds/{id}/dependencies": {
        "get": { "summary": "List builds this build depends on",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array of Build" } } }
      },
      "/builds/{id}/dependents": {
        "get": { "summary": "List builds that depend on this build",
          "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "$ref": "#/components/schemas/Uuid" } }],
          "responses": { "200": { "description": "Array of Build" } } }
      },
      "/auth/ldap": {
        "post": { "summary": "Authenticate via LDAP bind (sets session cookie)",
          "responses": {
            "200": { "description": "Authenticated; Set-Cookie returned" },
            "401": { "description": "Bad credentials" }
          } }
      },
      "/channel/{name}/git-revision": {
        "get": { "summary": "Plain-text git revision pinned to this channel",
          "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
          "responses": { "200": { "description": "Git revision",
            "content": { "text/plain": { "schema": { "type": "string" } } } } } }
      },
      "/channel/{name}/binary-cache-url": {
        "get": { "summary": "Plain-text binary cache URL for this channel",
          "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
          "responses": { "200": { "description": "Cache URL",
            "content": { "text/plain": { "schema": { "type": "string" } } } } } }
      },
      "/channel/{name}/store-paths.xz": {
        "get": { "summary": "xz-compressed list of channel store paths",
          "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
          "responses": { "200": { "description": "Compressed list" } } }
      },
      "/channel/{name}/nixexprs.tar.xz": {
        "get": { "summary": "Hydra-compatible nixexprs tarball for this channel",
          "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
          "responses": { "200": { "description": "xz-compressed tar containing channel/default.nix" } } }
      }
    }
  })
}

#[allow(clippy::unused_async)]
async fn openapi_spec() -> impl IntoResponse {
  (
    StatusCode::OK,
    [("content-type", "application/json")],
    document().to_string(),
  )
}

pub fn router() -> Router<AppState> {
  Router::new().route("/api/v1/openapi.json", get(openapi_spec))
}
