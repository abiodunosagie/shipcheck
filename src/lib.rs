//! # shipcheck
//!
//! Audits a website before launch: broken links and resources, link-preview tags (Open Graph and
//! Twitter cards), titles and descriptions, image alt text and size, headings, mixed content,
//! robots.txt, sitemap.xml and favicon.
//!
//! The single-page checks in [`page::analyze`] are pure functions over HTML, so they are tested
//! without a network. [`crawl::run`] crawls the site's own pages and verifies every link once.

pub mod crawl;
pub mod model;
pub mod page;
pub mod report;
pub mod serve;
