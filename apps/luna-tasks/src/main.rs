//! `luna-tasks` — menedżer procesów środowiska Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_tasks::run_tasks()
}
