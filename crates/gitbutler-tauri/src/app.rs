use std::{collections::HashMap, path};

use anyhow::{Context, Result};
use gitbutler_core::error::Error as CoreError;
use gitbutler_core::{
    gb_repository, git,
    project_repository::{self, conflicts},
    projects::{self, ProjectId},
    reader,
    sessions::{self, SessionId},
    users,
    virtual_branches::BranchId,
};

use crate::error::Error;

#[derive(Clone)]
pub struct App {
    local_data_dir: path::PathBuf,
    projects: projects::Controller,
    users: users::Controller,
    sessions_database: sessions::Database,
}

impl App {
    pub fn new(
        local_data_dir: path::PathBuf,
        projects: projects::Controller,
        users: users::Controller,
        sessions_database: sessions::Database,
    ) -> Self {
        Self {
            local_data_dir,
            projects,
            users,
            sessions_database,
        }
    }

    pub fn list_session_files(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
        paths: Option<&[&path::Path]>,
    ) -> Result<HashMap<path::PathBuf, reader::Content>, Error> {
        let session = self
            .sessions_database
            .get_by_project_id_id(project_id, session_id)
            .context("failed to get session")?
            .context("session not found")?;
        let user = self.users.get_user().context("failed to get user")?;
        let project = self
            .projects
            .get(project_id)
            .map_err(Error::from_error_with_context)?;
        let project_repository = project_repository::Repository::open(&project)
            .map_err(Error::from_error_with_context)?;
        let gb_repo = gb_repository::Repository::open(
            &self.local_data_dir,
            &project_repository,
            user.as_ref(),
        )
        .context("failed to open gb repository")?;
        let session_reader =
            sessions::Reader::open(&gb_repo, &session).context("failed to open session reader")?;
        Ok(session_reader
            .files(paths)
            .context("failed to read session files")?)
    }

    pub fn mark_resolved(&self, project_id: &ProjectId, path: &str) -> Result<(), CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        // mark file as resolved
        conflicts::resolve(&project_repository, path)?;
        Ok(())
    }

    pub fn git_remote_branches(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<git::RemoteRefname>, CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        Ok(project_repository.git_remote_branches()?)
    }

    pub fn git_test_push(
        &self,
        project_id: &ProjectId,
        remote_name: &str,
        branch_name: &str,
        credentials: &git::credentials::Helper,
        askpass: Option<Option<BranchId>>,
    ) -> Result<(), CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        Ok(project_repository.git_test_push(credentials, remote_name, branch_name, askpass)?)
    }

    pub fn git_test_fetch(
        &self,
        project_id: &ProjectId,
        remote_name: &str,
        credentials: &git::credentials::Helper,
        askpass: Option<String>,
    ) -> Result<(), CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        Ok(project_repository.fetch(remote_name, credentials, askpass)?)
    }

    pub fn git_index_size(&self, project_id: &ProjectId) -> Result<usize, CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        let size = project_repository
            .git_index_size()
            .context("failed to get index size")?;
        Ok(size)
    }

    pub fn git_head(&self, project_id: &ProjectId) -> Result<String, CoreError> {
        let project = self.projects.get(project_id)?;
        let project_repository = project_repository::Repository::open(&project)?;
        let head = project_repository
            .get_head()
            .context("failed to get repository head")?;
        Ok(head.name().unwrap().to_string())
    }

    pub fn git_set_global_config(key: &str, value: &str) -> Result<String> {
        let mut config = git2::Config::open_default()?;
        config.set_str(key, value)?;
        Ok(value.to_string())
    }

    pub fn git_get_global_config(key: &str) -> Result<Option<String>> {
        let config = git2::Config::open_default()?;
        let value = config.get_string(key);
        match value {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                if e.code() == git2::ErrorCode::NotFound {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    pub async fn delete_all_data(&self) -> Result<(), CoreError> {
        for project in self.projects.list().context("failed to list projects")? {
            self.projects
                .delete(&project.id)
                .await
                .map_err(|err| err.context("failed to delete project"))?;
        }
        Ok(())
    }
}
