mod app;
mod config;
mod fs;
mod ui;
mod git;

use anyhow::Result;
use app::App;
use config::Config;

fn main() -> Result<()> {
    let config = Config::load_or_create()?;
    let mut app = App::new(config)?;
    app.run()
}

