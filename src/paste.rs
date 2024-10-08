use crate::comments::Comment;
use crate::utils;
use actix_identity::Identity;
use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Deserialize;
use sqlx::PgPool;
use tera::Tera;
// Data structure for creating a paste
#[derive(Deserialize)]
pub struct CreatePasteForm {
    title: String,
    content: String,
}

// Data structure for displaying a paste
#[derive(serde::Serialize, Debug)]
pub struct Paste {
    creator_username: String,
    title: String,
    content: String,
    created_at: i64, // Unix timestamp
    views: i32,
    comments: Vec<Comment>, // Comments related to the paste
}

#[get("/")]
pub async fn index(
    user: Option<Identity>,
    tera: web::Data<Tera>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let pastes_result =
        sqlx::query!("SELECT creator_username, title, content, created_at, views FROM pastes")
            .fetch_all(pool.get_ref())
            .await;

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);
    let default_datetime = 0; // Default Unix timestamp value

    match pastes_result {
        Ok(rows) => {
            let pastes: Vec<Paste> = rows
                .into_iter()
                .map(|row| Paste {
                    creator_username: row.creator_username,
                    title: row.title,
                    content: row.content,
                    created_at: row.created_at.unwrap_or(default_datetime) as i64,
                    views: row.views.unwrap_or(0),
                    comments: Vec::new(), // No comments here, as we're listing pastes
                })
                .collect();

            context.insert("pastes", &pastes);
        }
        Err(e) => {
            eprintln!("Error fetching pastes: {:?}", e);
            context.insert("pastes", &Vec::<Paste>::new());
        }
    }

    let rendered = tera.render("index.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[get("/paste/{title}")]
pub async fn view_paste(
    title: web::Path<String>,
    pool: web::Data<PgPool>,
    tera: web::Data<Tera>,
    user: Option<Identity>,
) -> impl Responder {
    let title = title.into_inner();

    // Increment the view count for the paste
    let update_views_result = sqlx::query!(
        "UPDATE pastes SET views = views + 1 WHERE title = $1",
        title
    )
    .execute(pool.get_ref())
    .await;

    if let Err(e) = update_views_result {
        eprintln!("Error updating view count: {:?}", e);
    }

    // Fetch paste details
    let paste_result = sqlx::query!(
        "SELECT creator_username, title, content, created_at, views FROM pastes WHERE title = $1",
        title
    )
    .fetch_optional(pool.get_ref())
    .await;

    // Fetch comments related to the paste
    let comments_result = sqlx::query_as!(
        Comment,
        "SELECT id, creator_username, content, paste_id, created_at FROM comments WHERE paste_id = (SELECT id FROM pastes WHERE title = $1)",
        title
    )
    .fetch_all(pool.get_ref())
    .await;

    let mut context = tera::Context::new();
    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    context.insert("user_status", &user_status);

    match paste_result {
        Ok(Some(paste)) => {
            let paste_data = Paste {
                creator_username: paste.creator_username,
                title: paste.title,
                content: paste.content,
                created_at: paste.created_at.unwrap_or(0) as i64,
                views: paste.views.unwrap_or(0),
                comments: comments_result.unwrap_or_else(|_| Vec::new()),
            };
            context.insert("paste", &paste_data);
            let rendered = tera.render("paste.html", &context).unwrap();
            HttpResponse::Ok().content_type("text/html").body(rendered)
        }
        Ok(None) => HttpResponse::NotFound().body("Paste not found"),
        Err(_) => HttpResponse::InternalServerError().body("Error fetching paste"),
    }
}

#[get("/create")]
pub async fn create_form(user: Option<Identity>, tera: web::Data<Tera>) -> impl Responder {
    if user.is_none() {
        // Redirect to login if not logged in
        return HttpResponse::Found()
            .append_header(("Location", "/login"))
            .finish();
    }

    let user_status = user
        .and_then(|id| id.id().ok())
        .unwrap_or_else(|| "Anonymous".to_string());

    let mut context = tera::Context::new();
    context.insert("user_status", &user_status);

    let rendered = tera.render("create.html", &context).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

#[post("/create")]
pub async fn create_paste(
    user: Option<Identity>,
    form: web::Form<CreatePasteForm>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    let username = if let Some(user) = user {
        user.id().unwrap_or_else(|_| "Anonymous".to_string())
    } else {
        return HttpResponse::Found()
            .append_header(("Location", "/login"))
            .finish();
    };

    // Validate title
    if !utils::is_valid_title(&form.title) {
        return HttpResponse::BadRequest()
            .body("Invalid title characters, only use letters, numbers and _ . ( )");
    }

    // Check if a paste with the given title already exists
    let paste_exists = sqlx::query!(
        "SELECT EXISTS (SELECT 1 FROM pastes WHERE title = $1)",
        form.title
    )
    .fetch_one(pool.get_ref())
    .await
    .map_or(false, |row| row.exists.unwrap());

    if paste_exists {
        return HttpResponse::Conflict().body("Paste with this title already exists");
    }

    // Proceed to insert the new paste
    let result = sqlx::query!(
        "INSERT INTO pastes (creator_username, title, content) VALUES ($1, $2, $3)",
        username,
        form.title,
        form.content
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            // Redirect to the newly created paste
            HttpResponse::Found()
                .append_header(("Location", format!("/paste/{}", form.title)))
                .finish()
        }
        Err(sqlx::Error::Database(err)) if err.message().contains("unique_violation") => {
            HttpResponse::Conflict().body("Paste with this title already exists")
        }
        Err(_) => HttpResponse::InternalServerError().body("Error creating paste"),
    }
}
