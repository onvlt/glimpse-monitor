use glimpse_sdk::*;
use std::process::ExitCode;

struct App;

enum Msg {}

impl glimpse_sdk::App for App {
    type Msg = Msg;

    fn update(&mut self, _ctx: &Ctx, msg: Msg) {
        match msg {}
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            indicator()
                .icon("application-x-addon-symbolic")
                .tooltip("glimpse-monitor")
                .text("glimpse-monitor")
                .into(),
            popover()
                .icon("application-x-addon-symbolic")
                .title("glimpse-monitor")
                .child(section().title("glimpse-monitor").child(row().title("It works")))
                .into(),
        ]
    }
}

fn main() -> ExitCode {
    run(App)
}
