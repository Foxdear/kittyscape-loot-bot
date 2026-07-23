mod command_handler;
mod prices;
mod collection_log;
mod config;
mod rank_manager;
mod logger;
mod runescape_tracker;
mod dink_listener;

use anyhow::Result;
use axum::Extension;
use axum::RequestExt;
use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use serde_json::error::Category::Data;
use serenity::all::ChannelAction::Create;
use serenity::all::Http;
use serenity::all::{
    CreateAttachment, GatewayIntents, Interaction, Message, Ready
};
use sqlx::Sqlite;
use std::future::IntoFuture as _;
use serenity::async_trait;
use serenity::prelude::*;
use serenity::all::CreateMessage;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::env;
use std::fs::File;
use std::sync::Arc;
use dotenvy::dotenv;
use axum::{
    body::{Body, Bytes},
    extract::{Request, Json, Query, Multipart},
    http::{header::CONTENT_TYPE, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
    Form,
};
use serde_json::Value;
use tracing::{error, info, debug};
use command_handler::{PriceManagerKey, CollectionLogManagerKey};
use config::{Config, ConfigKey};
use runescape_tracker::RunescapeTrackerKey;
use serde::Deserialize;

struct Handler {
    db: SqlitePool,
    price_manager: Arc<prices::PriceManager>,
    collection_log_manager: Arc<collection_log::CollectionLogManager>,
    runescape_tracker: Arc<runescape_tracker::RunescapeTracker>,
}

#[derive(Clone)]
struct DinkHandler {
    db: SqlitePool,
    price_manager: Arc<prices::PriceManager>,
    collection_log_manager: Arc<collection_log::CollectionLogManager>,
    runescape_tracker: Arc<runescape_tracker::RunescapeTracker>,
    http: Arc<Http>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Err(why) = command_handler::handle_interaction(&ctx, &interaction, &self.db).await {
            error!("Error handling interaction: {:?}", why);
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // We only care about messages in the RuneLite plugin channel
        let data = ctx.data.read().await;
        if let Some(config) = data.get::<ConfigKey>() {
            if let Some(runelite_channel_id) = config.runelite_channel_id {
                if msg.channel_id == runelite_channel_id && msg.author.bot {
                    if let Err(why) = self.runescape_tracker.process_message(&ctx, &msg, &self.db).await {
                        error!("Error processing RuneLite message: {:?}", why);
                    }
                }
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        // Store managers in context data
        {
            let mut data = ctx.data.write().await;
            data.insert::<PriceManagerKey>(Arc::clone(&self.price_manager));
            data.insert::<CollectionLogManagerKey>(Arc::clone(&self.collection_log_manager));
            data.insert::<RunescapeTrackerKey>(Arc::clone(&self.runescape_tracker));
        }

        // Register commands
        if let Err(why) = command_handler::register_commands(&ctx).await {
            error!("Error registering commands: {:?}", why);
        }

        // Start price updates
        Arc::clone(&self.price_manager).start_price_updates().await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize environment variables
    dotenv()?;

    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("Starting bot...");

    // Get the token from the environment variable
    let token = env::var("DISCORD_TOKEN")?;
    let database_url = env::var("DATABASE_URL")?;

    // Initialize config
    let config = Config::from_env()?;

    // Create database connection pool
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Run migrations
    sqlx::migrate!().run(&db).await?;

    // Initialize managers
    let price_manager = Arc::new(prices::PriceManager::new().await?);
    let collection_log_manager = Arc::new(collection_log::CollectionLogManager::new(&db).await?);
    let runescape_tracker = Arc::new(runescape_tracker::RunescapeTracker::new().await?);

    // Create a new instance of the client
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_MESSAGES;
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            db: db.clone(),
            price_manager: Arc::clone(&price_manager),
            collection_log_manager: Arc::clone(&collection_log_manager),
            runescape_tracker: Arc::clone(&runescape_tracker),
        })
        .await?;

    // build our application with a single route
    let app = Router::new().route("/dinky", post(dink_handler).get(dink_handler))
    .route("/", get(|| async { "Hello, World!" }))
    .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
    .layer(Extension(DinkHandler {
            db: db.clone(),
            price_manager: Arc::clone(&price_manager),
            collection_log_manager: Arc::clone(&collection_log_manager),
            runescape_tracker: Arc::clone(&runescape_tracker),
            http: client.http.clone(),
        }));

    // Store config in client data
    {
        let mut data = client.data.write().await;
        data.insert::<ConfigKey>(config);
    }

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    let server = axum::serve(listener, app);

    tokio::select! {
        err = client.start() => tracing::warn!("Discord client quit: {err:?}"),
        err = server.into_future() => tracing::warn!("Axum server quit: {err:?}"),
    }  

    Ok(())
} 