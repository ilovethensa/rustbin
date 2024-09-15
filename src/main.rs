use actix_files as fs;
use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, get, middleware::Logger, web, App, HttpServer};
use tera::Tera;

mod auth;
mod paste;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://postgres:postgres@localhost/rustbin")
        .await
        .unwrap();

    sqlx::migrate!().run(&pool).await.unwrap();

    let tera = Tera::new("templates/**/*").unwrap();
    let secret_key = Key::generate();

    HttpServer::new(move || {
        App::new()
            .wrap(IdentityMiddleware::default())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                    .cookie_secure(false)
                    .build(),
            )
            .wrap(Logger::default())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(tera.clone()))
            .service(auth::login_form)
            .service(auth::register_form)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::logout_api)
            .service(auth::register)
            .service(paste::create_form)
            .service(paste::create_paste)
            .service(paste::view_paste)
            .service(paste::index)
            .service(
                fs::Files::new("/static", "./static")
                    .show_files_listing()
                    .use_last_modified(true),
            )
    })
    .bind("127.0.0.1:8080")?
    .workers(2)
    .run()
    .await
}
