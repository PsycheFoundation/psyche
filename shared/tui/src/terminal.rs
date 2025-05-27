use std::{
    io::{self, Write},
    panic,
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::{Backend, CrosstermBackend},
    Terminal,
};
use tracing::{error, trace};

pub struct TerminalWrapper<T: Backend>(pub Terminal<T>);

pub fn init_terminal() -> io::Result<TerminalWrapper<impl Backend>> {
    trace!(target:"crossterm", "Initializing terminal");
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    // panic messages are getting printed to the alt screen, which is cleared. cringe.
    let default_panic = std::panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        io::stdout().flush().unwrap();
        default_panic(info);
        io::stdout().flush().unwrap();
    }));

    Ok(TerminalWrapper(terminal))
}

impl<T: Backend> Drop for TerminalWrapper<T> {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    trace!(target:"crossterm", "Restoring terminal");
    if let Err(err) = disable_raw_mode() {
        error!("failed to disable terminal raw mode: {err:#}");
    }
    if let Err(err) = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture) {
        error!("failed to leave alternate screen & disable mouse capture: {err:#}");
    }
}
