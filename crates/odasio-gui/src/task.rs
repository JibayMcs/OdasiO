//! Travaux longs (analyse, conversion) exécutés hors du fil de l'interface.

use std::sync::{Arc, Mutex};

use eframe::egui;

pub enum State<T> {
    Running {
        /// Avancement entre 0 et 1, `None` si indéterminé.
        progress: Option<f32>,
        message: String,
    },
    Done(T),
    Failed(String),
}

/// Poignée partagée entre l'interface et le fil de travail.
pub struct Task<T> {
    state: Arc<Mutex<State<T>>>,
}

impl<T> Clone for Task<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

/// Côté fil de travail : publie l'avancement et réveille l'interface.
pub struct Reporter<T> {
    task: Task<T>,
    ctx: egui::Context,
}

impl<T> Reporter<T> {
    pub fn progress(&self, progress: Option<f32>, message: impl Into<String>) {
        *self.task.state.lock().unwrap() = State::Running {
            progress,
            message: message.into(),
        };
        self.ctx.request_repaint();
    }
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(
        ctx: &egui::Context,
        message: &str,
        work: impl FnOnce(&Reporter<T>) -> anyhow::Result<T> + Send + 'static,
    ) -> Self {
        let task = Task {
            state: Arc::new(Mutex::new(State::Running {
                progress: None,
                message: message.to_string(),
            })),
        };
        let reporter = Reporter {
            task: task.clone(),
            ctx: ctx.clone(),
        };
        std::thread::spawn(move || {
            let result = work(&reporter);
            *reporter.task.state.lock().unwrap() = match result {
                Ok(value) => State::Done(value),
                Err(e) => State::Failed(format!("{e:#}")),
            };
            reporter.ctx.request_repaint();
        });
        task
    }

    pub fn with<R>(&self, f: impl FnOnce(&State<T>) -> R) -> R {
        f(&self.state.lock().unwrap())
    }

    pub fn is_running(&self) -> bool {
        self.with(|s| matches!(s, State::Running { .. }))
    }

    /// Récupère le résultat une fois terminé (la tâche reste consultable via `with`).
    pub fn take_done(&self) -> Option<T>
    where
        T: Clone,
    {
        self.with(|s| match s {
            State::Done(v) => Some(v.clone()),
            _ => None,
        })
    }
}
