//! The applet itself: parses options once at start, keeps the sampling thread's
//! snapshot, and renders on every wake.

use crate::config::{self, Config};
use crate::monitor;
use crate::snapshot::Shared;
use crate::ui;
use glimpse_sdk::*;
use std::sync::Arc;

/// No event handlers yet: wakes drive every rebuild.
#[derive(Debug)]
pub enum Msg {}

#[derive(Default)]
pub struct App {
    config: Option<Result<Config, Vec<String>>>,
    shared: Option<Arc<Shared>>,
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl glimpse_sdk::App for App {
    type Msg = Msg;

    fn start(&mut self, ctx: &Ctx) {
        self.config = Some(config::parse(&ctx.options));
        if let Some(Ok(config)) = self.config.as_ref() {
            self.shared = Some(monitor::spawn(ctx.waker(), config.clone()));
        }
    }

    fn update(&mut self, _ctx: &Ctx, msg: Msg) {
        match msg {}
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        ui::tree(
            &self.config.clone().unwrap_or(Ok(Config::default())),
            self.shared.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::channel;

    fn hello(options: serde_json::Value) -> serde_json::Value {
        json!({
            "t": "hello", "v": 1,
            "name": "test", "state": "",
            "options": options,
            "placement": null,
        })
    }

    /// Runs one mount through the SDK session and returns the tree ops the
    /// panel would receive, exactly like a live handshake.
    fn mounted_ops(options: serde_json::Value) -> Vec<serde_json::Value> {
        let (tx, rx) = channel();
        let mut session = Session::new(App::new(), tx);
        session.handle(&hello(options));
        session.flush().expect("mount flush");

        let mut ops = Vec::new();
        while let Ok(In::Send(line)) = rx.try_recv() {
            let value: serde_json::Value =
                serde_json::from_str(&line).expect("sent line parses");
            if value["t"] == "commit" {
                ops.extend(value["ops"].as_array().cloned().unwrap_or_default());
            }
        }
        ops
    }

    fn inserted(ops: &[serde_json::Value]) -> Vec<&serde_json::Value> {
        ops.iter()
            .filter(|op| op["op"] == "insert")
            .map(|op| &op["node"])
            .collect()
    }

    #[test]
    fn default_options_mount_chips_and_a_popover() {
        let ops = mounted_ops(json!({}));
        let inserted = inserted(&ops);
        assert_eq!(inserted.len(), 5); // 4 chips + popover
        let kinds: Vec<&str> = inserted.iter().map(|node| node["type"].as_str().expect("type")).collect();
        assert_eq!(kinds, ["indicator", "indicator", "indicator", "indicator", "popover"]);
    }

    #[test]
    fn invalid_options_mount_only_the_error_chip() {
        let ops = mounted_ops(json!({ "interval": 0 }));
        let inserted = inserted(&ops);
        assert_eq!(inserted.len(), 1);
        let chip = inserted[0];
        assert_eq!(chip["type"], "indicator");
        assert_eq!(chip["props"]["severity"], json!("error"));
        assert_eq!(chip["props"]["text"], json!("invalid settings"));
        assert!(
            chip["props"]["tooltip"].as_str().is_some_and(|tooltip| tooltip.contains("interval")),
            "the tooltip explains the failure"
        );
    }
}
