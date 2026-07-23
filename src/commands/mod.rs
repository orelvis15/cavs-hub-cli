//! Subcommand implementations. Each module exposes a `run` entry point and,
//! where useful, an `Args` struct parsed by clap in `main`.

pub mod config;
pub mod doctor;
pub mod gitwrap;
pub mod hub;
pub mod index;
pub mod install_lfs;
pub mod login;
pub mod logout;
pub mod repo;
pub mod status;
pub mod transfer;
pub mod whoami;
