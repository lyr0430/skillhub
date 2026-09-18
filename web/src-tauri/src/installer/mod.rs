pub mod agents;
pub mod attach;
pub mod error;
pub mod frontmatter;
pub mod homepage;
pub mod install;
pub mod link;
pub mod local_skills;
pub mod metadata;
pub mod storage_path;

#[cfg(test)]
mod attach_tests;
#[cfg(test)]
mod install_mode_tests;
#[cfg(test)]
mod installer_tests;
#[cfg(test)]
mod local_skill_tests;
#[cfg(test)]
mod repo_display_tests;
#[cfg(test)]
mod storage_path_tests;
#[cfg(test)]
mod test_support;
