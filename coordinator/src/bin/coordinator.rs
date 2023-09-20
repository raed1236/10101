use anyhow::Context;
use anyhow::Result;
use coordinator::cli::Opts;
use coordinator::logger;
use coordinator::message::spawn_delivering_messages_to_authenticated_users;
use coordinator::message::NewUserMessage;
use coordinator::metrics;
use coordinator::metrics::init_meter;
use coordinator::node;
use coordinator::node::closed_positions;
use coordinator::node::connection;
use coordinator::node::expired_positions;
use coordinator::node::rollover;
use coordinator::node::storage::NodeStorage;
use coordinator::node::unrealized_pnl;
use coordinator::node::Node;
use coordinator::notifications::query_and_send_position_notifications;
use coordinator::notifications::NotificationService;
use coordinator::orderbook::async_match;
use coordinator::orderbook::trading;
use coordinator::routes::router;
use coordinator::run_migration;
use coordinator::settings::Settings;
use diesel::r2d2;
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use lightning::util::events::Event;
use ln_dlc_node::scorer;
use ln_dlc_node::seed::Bip39Seed;
use ln_dlc_node::CoordinatorEventHandler;
use rand::thread_rng;
use rand::RngCore;
use std::backtrace::Backtrace;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio::task::spawn_blocking;
use tracing::metadata::LevelFilter;

const PROCESS_PROMETHEUS_METRICS: Duration = Duration::from_secs(10);
const PROCESS_INCOMING_DLC_MESSAGES_INTERVAL: Duration = Duration::from_millis(200);
const EXPIRED_POSITION_SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);
const CLOSED_POSITION_SYNC_INTERVAL: Duration = Duration::from_secs(30);
const UNREALIZED_PNL_SYNC_INTERVAL: Duration = Duration::from_secs(10 * 60);
const CONNECTION_CHECK_INTERVAL: Duration = Duration::from_secs(30);
/// How often to check for expiring/expired positions to send push notifications for.
/// This should be configured in conjunction with the time windows of
/// expiring/expired notifications, ideally a bit less than the time window
/// (e.g. 58min for a 1h time window).
const POSITION_PUSH_NOTIFICATION_INTERVAL: Duration = Duration::from_secs(58 * 60);

const NODE_ALIAS: &str = "10101.finance";

#[tokio::main]
async fn main() -> Result<()> {
    std::panic::set_hook(
        #[allow(clippy::print_stderr)]
        Box::new(|info| {
            let backtrace = Backtrace::force_capture();

            tracing::error!(%info, "Aborting after panic in task");
            eprintln!("{backtrace}");

            std::process::abort()
        }),
    );

    let exporter = init_meter();

    let opts = Opts::read();
    let data_dir = opts.data_dir()?;
    let address = opts.p2p_address;
    let http_address = opts.http_address;
    let network = opts.network();

    logger::init_tracing(LevelFilter::DEBUG, opts.json, opts.tokio_console)?;

    let mut ephemeral_randomness = [0; 32];
    thread_rng().fill_bytes(&mut ephemeral_randomness);

    let data_dir = data_dir.join(network.to_string());
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .context(format!("Could not create data dir for {network}"))?;
    }

    let data_dir_string = data_dir.clone().into_os_string();
    tracing::info!("Data-dir: {data_dir_string:?}");

    let seed_path = data_dir.join("seed");
    let seed = Bip39Seed::initialize(&seed_path)?;

    let settings = Settings::new(&data_dir).await;

    // set up database connection pool
    let manager = ConnectionManager::<PgConnection>::new(opts.database.clone());
    let pool = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    let mut conn = pool.get()?;
    run_migration(&mut conn);

    let (node_event_sender, mut node_event_receiver) = watch::channel::<Option<Event>>(None);

    let node = Arc::new(ln_dlc_node::node::Node::new(
        ln_dlc_node::config::coordinator_config(),
        scorer::persistent_scorer,
        NODE_ALIAS,
        network,
        data_dir.as_path(),
        Arc::new(NodeStorage::new(pool.clone())),
        address,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), address.port()),
        opts.p2p_announcement_addresses(),
        opts.esplora.clone(),
        seed,
        ephemeral_randomness,
        settings.ln_dlc.clone(),
        opts.get_oracle_info().into(),
    )?);

    let event_handler = CoordinatorEventHandler::new(
        node.clone(),
        Some(node_event_sender),
        settings.ln_dlc.max_app_channel_size_sats,
    );
    let running = node.start(event_handler)?;
    let node = Node::new(node, running, pool.clone(), settings.to_node_settings());

    // TODO: Pass the tokio metrics into Prometheus
    if let Some(interval) = opts.tokio_metrics_interval_seconds {
        let handle = tokio::runtime::Handle::current();
        let runtime_monitor = tokio_metrics::RuntimeMonitor::new(&handle);
        let frequency = Duration::from_secs(interval);
        tokio::spawn(async move {
            for metrics in runtime_monitor.intervals() {
                tracing::debug!(?metrics, "tokio metrics");
                tokio::time::sleep(frequency).await;
            }
        });
    }

    std::thread::spawn(node.inner.sync_on_chain_wallet_periodically());

    tokio::spawn({
        let node = node.clone();
        async move {
            loop {
                let node = node.clone();
                spawn_blocking(move || node.process_incoming_dlc_messages())
                    .await
                    .expect("To spawn blocking thread");
                tokio::time::sleep(PROCESS_INCOMING_DLC_MESSAGES_INTERVAL).await;
            }
        }
    });

    tokio::spawn({
        let node = node.clone();
        async move {
            loop {
                match node_event_receiver.changed().await {
                    Ok(()) => {
                        let event = node_event_receiver.borrow().clone();
                        node::routing_fees::handle(node.clone(), event);
                    }
                    Err(e) => {
                        tracing::error!("Failed to receive event: {e:#}");
                    }
                }
            }
        }
    });

    tokio::spawn({
        let node = node.clone();
        async move {
            loop {
                let node = node.clone();
                spawn_blocking(move || metrics::collect(node))
                    .await
                    .expect("To spawn blocking thread");
                tokio::time::sleep(PROCESS_PROMETHEUS_METRICS).await;
            }
        }
    });

    tokio::spawn({
        let node = node.clone();
        async move {
            loop {
                tokio::time::sleep(UNREALIZED_PNL_SYNC_INTERVAL).await;
                if let Err(e) = unrealized_pnl::sync(node.clone()).await {
                    tracing::error!(
                        "Failed to sync unrealized PnL with positions in database: {e:#}"
                    );
                }
            }
        }
    });

    let (tx_user_feed, _rx) = broadcast::channel::<NewUserMessage>(100);

    let (tx_price_feed, _rx) = broadcast::channel(100);

    let (_handle, auth_users_notifier) =
        spawn_delivering_messages_to_authenticated_users(tx_user_feed.clone());

    let notification_service = NotificationService::new(opts.fcm_api_key.clone());

    let (_handle, trading_sender) = trading::start(
        pool.clone(),
        tx_price_feed.clone(),
        auth_users_notifier.clone(),
        notification_service.get_sender(),
        network,
    );
    let _handle = async_match::monitor(
        pool.clone(),
        tx_user_feed.clone(),
        auth_users_notifier.clone(),
        network,
    );
    let _handle = rollover::monitor(
        pool.clone(),
        tx_user_feed.clone(),
        auth_users_notifier,
        network,
    );

    tokio::spawn({
        let node = node.clone();
        let trading_sender = trading_sender.clone();
        async move {
            loop {
                tokio::time::sleep(EXPIRED_POSITION_SYNC_INTERVAL).await;
                if let Err(e) = expired_positions::close(node.clone(), trading_sender.clone()).await
                {
                    tracing::error!("Failed to close expired positions! Error: {e:#}");
                }
            }
        }
    });

    tokio::spawn({
        let node = node.clone();
        async move {
            loop {
                tokio::time::sleep(CLOSED_POSITION_SYNC_INTERVAL).await;
                if let Err(e) = closed_positions::sync(node.clone()) {
                    tracing::error!("Failed to sync closed DLCs with positions in database: {e:#}");
                }
            }
        }
    });

    tokio::spawn({
        let node = node.clone();
        connection::keep_public_channel_peers_connected(node.inner, CONNECTION_CHECK_INTERVAL)
    });

    let app = router(
        node,
        pool.clone(),
        settings,
        exporter,
        opts.p2p_announcement_addresses(),
        NODE_ALIAS,
        trading_sender,
        tx_price_feed,
        tx_user_feed,
    );

    tokio::spawn({
        let sender = notification_service.get_sender();
        let pool = pool.clone();
        async move {
            loop {
                tracing::debug!("Running expiring/expired position push notification task");
                match pool.get() {
                    Ok(mut conn) => {
                        if let Err(e) =
                            query_and_send_position_notifications(&mut conn, &sender).await
                        {
                            tracing::error!("Failed to send notifications: {e:#}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to get pool connection. Error: {e:?}");
                    }
                }

                tokio::time::sleep(POSITION_PUSH_NOTIFICATION_INTERVAL).await;
            }
        }
    });

    // Start the metrics exporter
    autometrics::prometheus_exporter::init();

    tracing::debug!("listening on http://{}", http_address);
    axum::Server::bind(&http_address)
        .serve(app.into_make_service())
        .await?;

    tracing::trace!("Server has had been launched");

    Ok(())
}
