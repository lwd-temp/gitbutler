mod dispatchers;
mod events;
mod handlers;

pub use events::Event;

use anyhow::Result;
use crossbeam_channel::{select, unbounded};

use crate::{bookmarks, deltas, files, gb_repository, projects, search, sessions};

pub struct Watcher<'watcher> {
    project_id: String,
    dispatcher: dispatchers::Dispatcher,
    handler: handlers::Handler<'watcher>,
    stop: crossbeam_channel::Receiver<()>,
}

impl<'watcher> Watcher<'watcher> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project: &projects::Project,
        project_store: projects::Storage,
        gb_repository: &'watcher gb_repository::Repository,
        deltas_searcher: search::Searcher,
        stop: crossbeam_channel::Receiver<()>,
        publisher: crossbeam_channel::Receiver<events::Event>,
        events_sender: crate::events::Sender,
        sessions_database: sessions::Database,
        deltas_database: deltas::Database,
        files_database: files::Database,
        bookmarks_database: bookmarks::Database,
    ) -> Result<Self> {
        Ok(Self {
            project_id: project.id.clone(),
            dispatcher: dispatchers::Dispatcher::new(
                project.id.clone(),
                project.path.clone(),
                publisher,
            ),
            handler: handlers::Handler::new(
                project.id.clone(),
                project_store,
                gb_repository,
                deltas_searcher,
                events_sender,
                sessions_database,
                deltas_database,
                files_database,
                bookmarks_database,
            ),
            stop,
        })
    }

    pub fn start(&self) -> Result<()> {
        let (events_tx, events_rx) = unbounded();
        let dispatcher = self.dispatcher.clone();
        let project_id = self.project_id.clone();
        let etx = events_tx.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = dispatcher.start(etx.clone()).await {
                log::error!("{}: failed to start dispatcher: {:#}", project_id, e);
            }
        });

        loop {
            select! {
                recv(events_rx) -> event => match event {
                    Ok(event) => {
                        match self.handler.handle(event) {
                            Ok(events) => {
                                for event in events {
                                    if let Err(e) = events_tx.send(event) {
                                        log::error!("{}: failed to post event: {:#}", self.project_id, e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("{}: failed to handle event: {:#}", self.project_id, e);
                            }
                        }
                    },
                    Err(e) => {
                        log::error!("{}: failed to receive event: {:#}", self.project_id, e);
                    }
                },
                recv(self.stop) -> _ => {
                    if let Err(e) = self.dispatcher.stop() {
                        log::error!("{}: failed to stop dispatcher : {:#}", self.project_id, e);
                    }
                    break;
                }
            }
        }

        Ok(())
    }
}
