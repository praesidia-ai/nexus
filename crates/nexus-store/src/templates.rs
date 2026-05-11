//! Starter templates — pre-built app configurations for the consumer experience.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

/// (name, description, category, icon, preview_prompt, features, estimated_credits)
type TemplateSeed<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str, &'a [&'a str], i32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarterTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub preview_prompt: String,
    pub features: Vec<String>,
    pub estimated_credits: i32,
    pub popularity: i32,
    pub created_at: String,
}

pub struct TemplateService<'c> {
    conn: &'c Connection,
}

impl<'c> TemplateService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Seeds default starter templates (idempotent — only seeds if table is empty).
    pub fn seed_defaults(&self) -> Result<()> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM starter_templates", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        let templates: Vec<TemplateSeed<'_>> = vec![
            (
                "Online Store",
                "Full e-commerce store with product catalog, shopping cart, checkout flow, and order management. Includes inventory tracking and customer accounts.",
                "business",
                "shopping-cart",
                "Build a complete online store with a product catalog showing items in a grid with images, prices, and descriptions. Include a shopping cart that persists across pages, a multi-step checkout flow with shipping and payment forms, and an order management dashboard. Add inventory tracking, customer account pages with order history, and a simple admin panel to manage products. Use a clean, modern design with a hero section and featured products on the homepage.",
                &["Product catalog", "Shopping cart", "Checkout flow", "Order management", "Customer accounts", "Inventory tracking"],
                15,
            ),
            (
                "Portfolio Website",
                "Professional portfolio to showcase your work, skills, and experience. Features project galleries, about section, and contact form.",
                "personal",
                "briefcase",
                "Create a professional portfolio website with a striking hero section featuring name and tagline, an about page with bio and skills visualization, a projects gallery with filterable categories showing screenshots and descriptions, a resume/experience timeline, testimonials section, and a contact form with email integration. Include smooth scroll animations and a dark/light mode toggle. Make it responsive and visually impressive.",
                &["Project gallery", "Skills section", "Contact form", "Resume timeline", "Testimonials", "Dark/light mode"],
                8,
            ),
            (
                "SaaS Dashboard",
                "Analytics dashboard for SaaS products with charts, metrics, user management, and subscription tracking.",
                "business",
                "bar-chart",
                "Build a SaaS analytics dashboard with a sidebar navigation, a main dashboard page showing KPI cards (MRR, active users, churn rate, NPS), interactive charts for revenue over time, user growth, and feature usage. Include a user management table with search, filters, and pagination, a subscription management page showing plans and billing, a settings page, and a notifications panel. Use a professional design with consistent chart styling.",
                &["KPI dashboard", "Interactive charts", "User management", "Subscription tracking", "Settings page", "Notifications"],
                12,
            ),
            (
                "Blog & Newsletter",
                "Content publishing platform with a blog, newsletter signup, RSS feed, and content management. Perfect for writers and creators.",
                "content",
                "pen-tool",
                "Create a blog and newsletter platform with a homepage showing recent posts in a card layout, individual post pages with rich text rendering, a category and tag system, a newsletter signup form with email capture, an RSS feed, a search page, and an author profile section. Include a simple admin interface for creating and editing posts with a markdown editor. Add reading time estimates and social sharing buttons.",
                &["Blog posts", "Newsletter signup", "RSS feed", "Categories & tags", "Search", "Markdown editor"],
                8,
            ),
            (
                "Booking System",
                "Appointment scheduling system with calendar views, time slot management, reminders, and client management.",
                "business",
                "calendar",
                "Build an appointment booking system with a public booking page showing available time slots in a weekly calendar view, a service selection step where users pick what they want to book, a booking confirmation flow with email notifications, a client-facing dashboard to view and manage their appointments, and an admin panel with a full calendar view, client list, and availability settings. Include reminder notifications and the ability to block off unavailable times.",
                &["Calendar view", "Time slot management", "Booking flow", "Client dashboard", "Email reminders", "Availability settings"],
                12,
            ),
            (
                "Project Management",
                "Kanban-style project management tool with boards, tasks, team assignments, and progress tracking.",
                "productivity",
                "kanban",
                "Create a project management application with a kanban board featuring draggable task cards across columns (To Do, In Progress, Review, Done), task detail modals with description, assignee, due date, labels, and comments, a list view alternative, project-level settings and member management, a timeline/Gantt view showing task dependencies, and a dashboard with project progress metrics. Include task filtering, search, and the ability to create multiple boards per project.",
                &["Kanban board", "Task management", "Team assignments", "Timeline view", "Progress tracking", "Multiple boards"],
                15,
            ),
            (
                "Restaurant Menu & Orders",
                "Digital menu and ordering system for restaurants with menu management, table ordering, and kitchen display.",
                "business",
                "utensils",
                "Build a restaurant digital menu and ordering system with a beautiful public-facing menu organized by categories (appetizers, mains, desserts, drinks) with item photos and descriptions, a table-side ordering flow where customers can add items to their order and submit, a kitchen display showing incoming orders in real-time, an order status tracker for customers, and an admin panel for managing menu items, prices, and categories. Include dietary labels (vegan, gluten-free) and a daily specials section.",
                &["Digital menu", "Table ordering", "Kitchen display", "Order tracking", "Menu management", "Dietary labels"],
                12,
            ),
            (
                "Community Forum",
                "Discussion forum with threads, categories, user profiles, reputation system, and moderation tools.",
                "social",
                "message-circle",
                "Create a community forum with a homepage showing recent and popular threads, category-based organization, a thread view with nested replies and reactions, user profiles with avatar, bio, and post history, a reputation/karma system based on upvotes, a rich text editor for composing posts, search with filters, and moderation tools (pin, lock, delete threads). Include user badges, a notification system for replies and mentions, and a trending topics sidebar.",
                &["Discussion threads", "Categories", "User profiles", "Reputation system", "Moderation tools", "Notifications"],
                15,
            ),
            (
                "Fitness Tracker",
                "Personal fitness tracking app with workout logging, progress charts, exercise library, and goal setting.",
                "health",
                "activity",
                "Build a fitness tracker application with a dashboard showing today's stats (calories, steps, workouts), a workout logging interface where users select exercises from a library and log sets/reps/weight, progress charts showing strength and cardio improvements over time, a goal setting system with visual progress bars, a workout history calendar, and an exercise library with descriptions and muscle group tags. Include weekly summary reports and personal records tracking.",
                &["Workout logging", "Progress charts", "Exercise library", "Goal setting", "Workout history", "Personal records"],
                10,
            ),
            (
                "Invoice Generator",
                "Professional invoice creation tool with templates, client management, payment tracking, and PDF export.",
                "business",
                "file-text",
                "Create an invoice generator with a clean invoice creation form (client details, line items with quantity and price, tax calculation, discounts), multiple professional invoice templates to choose from, a client management system with saved client details, invoice status tracking (draft, sent, paid, overdue), a dashboard showing outstanding and paid totals, and the ability to preview and export invoices as PDF. Include recurring invoice support and payment terms configuration.",
                &["Invoice creation", "Multiple templates", "Client management", "Payment tracking", "PDF export", "Recurring invoices"],
                10,
            ),
        ];

        for (name, description, category, icon, preview_prompt, features, estimated_credits) in templates {
            let id = Uuid::new_v4().to_string();
            let features_json = serde_json::to_string(&features).unwrap_or_else(|_| "[]".to_string());
            self.conn.execute(
                "INSERT INTO starter_templates (id, name, description, category, icon, preview_prompt, features, estimated_credits)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, name, description, category, icon, preview_prompt, features_json, estimated_credits],
            )?;
        }
        Ok(())
    }

    /// List all starter templates ordered by popularity descending.
    pub fn list_templates(&self) -> Result<Vec<StarterTemplate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, category, icon, preview_prompt, features, estimated_credits, popularity, created_at
             FROM starter_templates ORDER BY popularity DESC, name ASC",
        )?;
        let rows = stmt.query_map([], Self::row_to_template)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Get a single template by ID.
    pub fn get_template(&self, id: &str) -> Result<Option<StarterTemplate>> {
        let result = self.conn.query_row(
            "SELECT id, name, description, category, icon, preview_prompt, features, estimated_credits, popularity, created_at
             FROM starter_templates WHERE id = ?1",
            params![id],
            Self::row_to_template,
        );
        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Increment the popularity counter for a template (called when a user picks it).
    pub fn increment_popularity(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE starter_templates SET popularity = popularity + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    fn row_to_template(r: &rusqlite::Row) -> rusqlite::Result<StarterTemplate> {
        let features_str: String = r.get(6)?;
        Ok(StarterTemplate {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            category: r.get(3)?,
            icon: r.get(4)?,
            preview_prompt: r.get(5)?,
            features: serde_json::from_str(&features_str).unwrap_or_default(),
            estimated_credits: r.get(7)?,
            popularity: r.get(8)?,
            created_at: r.get(9)?,
        })
    }
}
