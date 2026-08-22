//! User-facing web UI — server-rendered HTML pages (M4 first cut).
//!
//! See H-54 for the stack decision (askama + axum, no JS framework, no build
//! step). All pages are mounted at `/` in `lib.rs` and live alongside the
//! existing JSON API at `/auth/`, `/supervisors/`, etc. — the web pages are
//! thin wrappers that call into the same business logic.
//!
//! MVP surface (H-54 scope = user):
//!   `GET  /`                  landing page
//!   `GET  /register`          registration form
//!   `POST /register`          create account, set JWT cookie, redirect `/me`
//!   `GET  /login`             login form
//!   `POST /login`             verify, set JWT cookie, redirect `/me`
//!   `POST /logout`            clear cookie, redirect `/`
//!   `GET  /me`                current user (requires session)
//!   `GET  /supervisors`       search form + results
//!   `GET  /supervisors/new`   create-supervisor form (requires session)
//!   `POST /supervisors/new`   submit supervisor, redirect to detail
//!   `GET  /supervisors/{alias}` public view (alias → aggregate + radar)
//!   `GET  /supervisors/{alias}/rate`  rating form (requires session)
//!   `POST /supervisors/{alias}/rate`  submit rating, redirect to detail

pub mod session;
pub mod auth_pages;
pub mod supervisor_pages;
pub mod rating_pages;
pub mod templates;
pub mod router;
