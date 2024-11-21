#![deny(clippy::all)]

use actix_web::{middleware::Logger, App, HttpServer};
use clap::{arg, builder::ValueParser, value_parser, ArgAction, Command};
use std::{collections::HashSet, ffi::OsString};
use taskchampion_sync_server::WebServer;
use taskchampion_sync_server_core::ServerConfig;
use taskchampion_sync_server_storage_sqlite::SqliteStorage;
use uuid::Uuid;

fn command() -> Command {
    let defaults = ServerConfig::default();
    let default_snapshot_versions = defaults.snapshot_versions.to_string();
    let default_snapshot_days = defaults.snapshot_days.to_string();
    Command::new("taskchampion-sync-server")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Server for TaskChampion")
        .arg(
            arg!(-p --port <PORT> "Port on which to serve")
                .help("Port on which to serve")
                .value_parser(value_parser!(usize))
                .default_value("8080"),
        )
        .arg(
            arg!(-d --"data-dir" <DIR> "Directory in which to store data")
                .value_parser(ValueParser::os_string())
                .default_value("/var/lib/taskchampion-sync-server"),
        )
        .arg(
            arg!(-C --"allow-client-id" <CLIENT_IDS> "Client IDs to allow (can be repeated; default: all)")
                .value_parser(value_parser!(Uuid))
                .action(ArgAction::Append)
                .required(false),
        )
        .arg(
            arg!(--"snapshot-versions" <NUM> "Target number of versions between snapshots")
                .value_parser(value_parser!(u32))
                .default_value(default_snapshot_versions),
        )
        .arg(
            arg!(--"snapshot-days" <NUM> "Target number of days between snapshots")
                .value_parser(value_parser!(i64))
                .default_value(default_snapshot_days),
        )
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let matches = command().get_matches();

    let data_dir: &OsString = matches.get_one("data-dir").unwrap();
    let port: usize = *matches.get_one("port").unwrap();
    let snapshot_versions: u32 = *matches.get_one("snapshot-versions").unwrap();
    let snapshot_days: i64 = *matches.get_one("snapshot-days").unwrap();
    let client_id_allowlist: Option<HashSet<Uuid>> = matches
        .get_many("allow-client-id")
        .map(|ids| ids.copied().collect());

    let config = ServerConfig {
        snapshot_days,
        snapshot_versions,
    };
    let server = WebServer::new(config, client_id_allowlist, SqliteStorage::new(data_dir)?);

    log::info!("Serving on port {}", port);
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .configure(|cfg| server.config(cfg))
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::{self, App};
    use clap::ArgMatches;
    use taskchampion_sync_server_core::InMemoryStorage;

    /// Get the list of allowed client IDs
    fn allowed(matches: &ArgMatches) -> Option<Vec<Uuid>> {
        matches
            .get_many::<Uuid>("allow-client-id")
            .map(|ids| ids.copied().collect::<Vec<_>>())
    }

    #[test]
    fn command_allowed_client_ids_none() {
        let matches = command().get_matches_from(["tss"]);
        assert_eq!(allowed(&matches), None);
    }

    #[test]
    fn command_allowed_client_ids_one() {
        let matches =
            command().get_matches_from(["tss", "-C", "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0"]);
        assert_eq!(
            allowed(&matches),
            Some(vec![Uuid::parse_str(
                "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0"
            )
            .unwrap()])
        );
    }

    #[test]
    fn command_allowed_client_ids_two() {
        let matches = command().get_matches_from([
            "tss",
            "-C",
            "711d5cf3-0cf0-4eb8-9eca-6f7f220638c0",
            "-C",
            "bbaf4b61-344a-4a39-a19e-8caa0669b353",
        ]);
        assert_eq!(
            allowed(&matches),
            Some(vec![
                Uuid::parse_str("711d5cf3-0cf0-4eb8-9eca-6f7f220638c0").unwrap(),
                Uuid::parse_str("bbaf4b61-344a-4a39-a19e-8caa0669b353").unwrap()
            ])
        );
    }

    #[test]
    fn command_data_dir() {
        let matches = command().get_matches_from(["tss", "--data-dir", "/foo/bar"]);
        assert_eq!(matches.get_one::<OsString>("data-dir").unwrap(), "/foo/bar");
    }

    #[actix_rt::test]
    async fn test_index_get() {
        let server = WebServer::new(Default::default(), None, InMemoryStorage::new());
        let app = App::new().configure(|sc| server.config(sc));
        let app = actix_web::test::init_service(app).await;

        let req = actix_web::test::TestRequest::get().uri("/").to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
