//! Product Completeness Engine — generates realistic, domain-specific content
//! that makes every app feel like a real product, not a template.
//!
//! Implements all 5 product systems:
//! 1. Product Completeness — realistic seed data and copy
//! 2. Flow Engine — user journeys, CTAs, navigation
//! 3. Agent UX Integration — contextual AI triggers
//! 4. Micro-Delight System — animation classes and polish
//! 5. Hero Clarity Enforcement — benefit-driven headlines
//!
//! Everything is deterministic (no LLM). This runs during intent analysis
//! and feeds into the spec generation prompt so the LLM has rich context.

use crate::intent_engine::{AppType, FlatIntent, UiStyle};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Output type — everything the pipeline needs to generate a complete product
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductBrief {
    /// Domain (e.g., "wine", "fitness", "saas")
    pub domain: String,
    /// Hero section content
    pub hero: HeroContent,
    /// Key user flows
    pub flows: Vec<UserFlow>,
    /// Seed data for populating the app
    pub seed_content: Vec<ContentSection>,
    /// Agent integration hints
    pub agent_placements: Vec<AgentPlacement>,
    /// Animation/delight classes to apply
    pub delight_classes: DelightConfig,
    /// Navigation structure
    pub nav_items: Vec<NavItem>,
    /// Social proof / trust signals
    pub trust_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroContent {
    pub headline: String,
    pub subheadline: String,
    pub cta_text: String,
    pub cta_href: String,
    /// Optional secondary CTA
    pub secondary_cta: Option<(String, String)>,
    /// Optional stat badges (e.g., "500+ wines", "4.9★ rating")
    pub badges: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFlow {
    pub name: String,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    pub label: String,
    pub route: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSection {
    pub section_id: String,
    pub title: String,
    pub items: Vec<ContentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    pub name: String,
    pub description: String,
    pub detail: String,
    pub price: Option<String>,
    pub badge: Option<String>,
    pub image_emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPlacement {
    pub agent_name: String,
    pub placement: String,
    pub trigger_text: String,
    pub context_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelightConfig {
    pub entrance_animation: String,
    pub card_hover: String,
    pub button_hover: String,
    pub section_reveal: String,
    pub hero_animation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavItem {
    pub label: String,
    pub href: String,
    pub is_cta: bool,
}

// ---------------------------------------------------------------------------
// Main generator
// ---------------------------------------------------------------------------

/// Generate a complete product brief from an intent analysis.
/// This is 100% deterministic — no LLM calls.
pub fn generate_product_brief(intent: &FlatIntent, description: &str) -> ProductBrief {
    let domain = detect_domain_from_intent(intent, description);

    let hero = generate_hero(&domain, intent, description);
    let flows = generate_flows(&domain, intent);
    let seed_content = generate_seed_content(&domain, intent);
    let agent_placements = generate_agent_placements(intent, &domain);
    let delight_classes = generate_delight_config(&intent.ui_style);
    let nav_items = generate_nav(intent, &domain);
    let trust_signals = generate_trust_signals(&domain);

    ProductBrief {
        domain: domain.clone(),
        hero,
        flows,
        seed_content,
        agent_placements,
        delight_classes,
        nav_items,
        trust_signals,
    }
}

// ---------------------------------------------------------------------------
// System 1: Product Completeness — seed content
// ---------------------------------------------------------------------------

fn generate_seed_content(domain: &str, _intent: &FlatIntent) -> Vec<ContentSection> {
    match domain {
        "wine" => vec![
            ContentSection {
                section_id: "featured".into(),
                title: "Featured Collection".into(),
                items: vec![
                    ContentItem {
                        name: "Chateau Margaux 2018".into(),
                        description: "Bordeaux, France".into(),
                        detail: "Velvety tannins with notes of blackcurrant, violet, and a hint of cedar. Exceptional aging potential.".into(),
                        price: Some("$189".into()),
                        badge: Some("Editor's Pick".into()),
                        image_emoji: "🍷".into(),
                    },
                    ContentItem {
                        name: "Opus One 2019".into(),
                        description: "Napa Valley, USA".into(),
                        detail: "A harmonious blend of Cabernet and Merlot. Rich dark fruit, silk texture, endless finish.".into(),
                        price: Some("$425".into()),
                        badge: Some("96 Points".into()),
                        image_emoji: "🍇".into(),
                    },
                    ContentItem {
                        name: "Cloudy Bay Sauvignon Blanc".into(),
                        description: "Marlborough, New Zealand".into(),
                        detail: "Crisp citrus and passionfruit, perfectly balanced acidity. Ideal for seafood pairings.".into(),
                        price: Some("$28".into()),
                        badge: None,
                        image_emoji: "🥂".into(),
                    },
                    ContentItem {
                        name: "Barolo Riserva 2016".into(),
                        description: "Piedmont, Italy".into(),
                        detail: "The king of wines. Rose petal, tar, and truffle aromas with extraordinary depth.".into(),
                        price: Some("$95".into()),
                        badge: Some("Limited".into()),
                        image_emoji: "🏰".into(),
                    },
                ],
            },
            ContentSection {
                section_id: "categories".into(),
                title: "Explore by Region".into(),
                items: vec![
                    ContentItem { name: "Bordeaux".into(), description: "France's legendary wine region".into(), detail: "142 wines".into(), price: None, badge: None, image_emoji: "🇫🇷".into() },
                    ContentItem { name: "Tuscany".into(), description: "Home of Chianti and Brunello".into(), detail: "89 wines".into(), price: None, badge: None, image_emoji: "🇮🇹".into() },
                    ContentItem { name: "Napa Valley".into(), description: "Bold Californian Cabernets".into(), detail: "76 wines".into(), price: None, badge: None, image_emoji: "🇺🇸".into() },
                    ContentItem { name: "Rioja".into(), description: "Spain's Tempranillo heartland".into(), detail: "54 wines".into(), price: None, badge: None, image_emoji: "🇪🇸".into() },
                ],
            },
        ],
        "saas" | "platform" | "tool" => vec![
            ContentSection {
                section_id: "features".into(),
                title: "Everything you need".into(),
                items: vec![
                    ContentItem { name: "Analytics Dashboard".into(), description: "Real-time insights at a glance".into(), detail: "Track key metrics, visualize trends, and export reports in one click.".into(), price: None, badge: None, image_emoji: "📊".into() },
                    ContentItem { name: "Team Collaboration".into(), description: "Work together seamlessly".into(), detail: "Shared workspaces, role-based access, and real-time commenting built in.".into(), price: None, badge: None, image_emoji: "👥".into() },
                    ContentItem { name: "Automations".into(), description: "Set it and forget it".into(), detail: "Trigger actions on events, schedule recurring tasks, and reduce manual work by 80%.".into(), price: None, badge: None, image_emoji: "⚡".into() },
                    ContentItem { name: "API & Integrations".into(), description: "Connect everything".into(), detail: "REST API, webhooks, and 50+ pre-built integrations with tools you already use.".into(), price: None, badge: None, image_emoji: "🔗".into() },
                ],
            },
            ContentSection {
                section_id: "pricing".into(),
                title: "Simple, transparent pricing".into(),
                items: vec![
                    ContentItem { name: "Starter".into(), description: "For individuals".into(), detail: "Up to 1,000 records, 1 user, email support".into(), price: Some("$0/mo".into()), badge: Some("Free".into()), image_emoji: "🌱".into() },
                    ContentItem { name: "Pro".into(), description: "For growing teams".into(), detail: "Unlimited records, 10 users, priority support, API access".into(), price: Some("$29/mo".into()), badge: Some("Popular".into()), image_emoji: "🚀".into() },
                    ContentItem { name: "Enterprise".into(), description: "For organizations".into(), detail: "Unlimited everything, SSO, dedicated support, custom integrations".into(), price: Some("Custom".into()), badge: None, image_emoji: "🏢".into() },
                ],
            },
        ],
        "restaurant" | "food" => vec![
            ContentSection {
                section_id: "menu".into(),
                title: "Our Menu".into(),
                items: vec![
                    ContentItem { name: "Seared Duck Breast".into(), description: "With cherry gastrique and roasted root vegetables".into(), detail: "Pasture-raised, 28-day aged".into(), price: Some("$34".into()), badge: Some("Chef's Choice".into()), image_emoji: "🦆".into() },
                    ContentItem { name: "Handmade Pappardelle".into(), description: "Wild mushroom ragù, truffle oil, aged Parmigiano".into(), detail: "Vegetarian".into(), price: Some("$26".into()), badge: None, image_emoji: "🍝".into() },
                    ContentItem { name: "Pan-Seared Branzino".into(), description: "Lemon caper beurre blanc, seasonal greens".into(), detail: "Sustainably sourced".into(), price: Some("$38".into()), badge: None, image_emoji: "🐟".into() },
                    ContentItem { name: "Chocolate Fondant".into(), description: "Molten center, vanilla bean ice cream, gold leaf".into(), detail: "Prepare 15 min".into(), price: Some("$16".into()), badge: Some("Must Try".into()), image_emoji: "🍫".into() },
                ],
            },
        ],
        "fitness" | "health" => vec![
            ContentSection {
                section_id: "programs".into(),
                title: "Training Programs".into(),
                items: vec![
                    ContentItem { name: "Strength Foundations".into(), description: "Build a solid base in 8 weeks".into(), detail: "3x/week • Beginner friendly • Equipment: dumbbells".into(), price: Some("$49".into()), badge: Some("Bestseller".into()), image_emoji: "💪".into() },
                    ContentItem { name: "HIIT & Burn".into(), description: "High-intensity fat loss program".into(), detail: "4x/week • Intermediate • No equipment needed".into(), price: Some("$39".into()), badge: None, image_emoji: "🔥".into() },
                    ContentItem { name: "Yoga Flow".into(), description: "Flexibility and mindfulness combined".into(), detail: "5x/week • All levels • Mat required".into(), price: Some("$29".into()), badge: None, image_emoji: "🧘".into() },
                ],
            },
        ],
        "ecommerce" | "shop" | "store" => vec![
            ContentSection {
                section_id: "products".into(),
                title: "New Arrivals".into(),
                items: vec![
                    ContentItem { name: "Essential Hoodie".into(), description: "Organic cotton, relaxed fit".into(), detail: "Available in 6 colors".into(), price: Some("$68".into()), badge: Some("New".into()), image_emoji: "👕".into() },
                    ContentItem { name: "Everyday Backpack".into(), description: "Water-resistant, laptop compartment".into(), detail: "30L capacity".into(), price: Some("$120".into()), badge: Some("Bestseller".into()), image_emoji: "🎒".into() },
                    ContentItem { name: "Wireless Earbuds Pro".into(), description: "Active noise cancellation, 8h battery".into(), detail: "IPX5 water resistant".into(), price: Some("$149".into()), badge: None, image_emoji: "🎧".into() },
                    ContentItem { name: "Minimal Watch".into(), description: "Sapphire crystal, Japanese movement".into(), detail: "Stainless steel case".into(), price: Some("$195".into()), badge: Some("Limited".into()), image_emoji: "⌚".into() },
                ],
            },
        ],
        "marketplace" => vec![
            ContentSection {
                section_id: "listings".into(),
                title: "Popular Listings".into(),
                items: vec![
                    ContentItem { name: "Modern Loft Downtown".into(), description: "Bright 2BR with skyline views".into(), detail: "Hosted by Sarah • ★ 4.95 • 128 reviews".into(), price: Some("$145/night".into()), badge: Some("Superhost".into()), image_emoji: "🏙️".into() },
                    ContentItem { name: "Cozy Mountain Cabin".into(), description: "Fireplace, hot tub, trail access".into(), detail: "Hosted by Mike • ★ 4.88 • 94 reviews".into(), price: Some("$112/night".into()), badge: None, image_emoji: "🏔️".into() },
                    ContentItem { name: "Beachfront Villa".into(), description: "Private pool, ocean view, sleeps 8".into(), detail: "Hosted by Elena • ★ 4.97 • 203 reviews".into(), price: Some("$289/night".into()), badge: Some("Guest Favorite".into()), image_emoji: "🏖️".into() },
                ],
            },
        ],
        _ => vec![
            ContentSection {
                section_id: "features".into(),
                title: "What we offer".into(),
                items: vec![
                    ContentItem { name: "Fast & Reliable".into(), description: "Built for speed".into(), detail: "Lightning-fast performance with 99.9% uptime guarantee.".into(), price: None, badge: None, image_emoji: "⚡".into() },
                    ContentItem { name: "Secure by Default".into(), description: "Your data is safe".into(), detail: "End-to-end encryption, SOC 2 compliant, regular security audits.".into(), price: None, badge: None, image_emoji: "🔒".into() },
                    ContentItem { name: "24/7 Support".into(), description: "We're here for you".into(), detail: "Real humans, average response time under 5 minutes.".into(), price: None, badge: None, image_emoji: "💬".into() },
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// System 2: Flow Engine — user journeys
// ---------------------------------------------------------------------------

fn generate_flows(_domain: &str, intent: &FlatIntent) -> Vec<UserFlow> {
    let mut flows = Vec::new();

    // Primary flow based on app type
    match intent.app_type {
        AppType::ECommerce => {
            flows.push(UserFlow {
                name: "Purchase".into(),
                steps: vec![
                    FlowStep { label: "Browse products".into(), route: "/products".into(), action: "view".into() },
                    FlowStep { label: "Add to cart".into(), route: "/cart".into(), action: "click".into() },
                    FlowStep { label: "Checkout".into(), route: "/checkout".into(), action: "submit".into() },
                ],
            });
        }
        AppType::SaasApp => {
            flows.push(UserFlow {
                name: "Onboarding".into(),
                steps: vec![
                    FlowStep { label: "Sign up".into(), route: "/register".into(), action: "submit".into() },
                    FlowStep { label: "View dashboard".into(), route: "/dashboard".into(), action: "view".into() },
                    FlowStep { label: "Create first item".into(), route: "/dashboard".into(), action: "click".into() },
                ],
            });
        }
        AppType::Marketplace => {
            flows.push(UserFlow {
                name: "Discovery".into(),
                steps: vec![
                    FlowStep { label: "Browse listings".into(), route: "/browse".into(), action: "view".into() },
                    FlowStep { label: "View details".into(), route: "/listing".into(), action: "click".into() },
                    FlowStep { label: "Book or contact".into(), route: "/booking".into(), action: "submit".into() },
                ],
            });
        }
        AppType::LandingPage => {
            flows.push(UserFlow {
                name: "Conversion".into(),
                steps: vec![
                    FlowStep { label: "Read value prop".into(), route: "/".into(), action: "scroll".into() },
                    FlowStep { label: "View features".into(), route: "/#features".into(), action: "scroll".into() },
                    FlowStep { label: "Sign up / Contact".into(), route: "/#cta".into(), action: "submit".into() },
                ],
            });
        }
        _ => {
            flows.push(UserFlow {
                name: "Explore".into(),
                steps: vec![
                    FlowStep { label: "View home".into(), route: "/".into(), action: "view".into() },
                    FlowStep { label: "Browse content".into(), route: "/browse".into(), action: "click".into() },
                ],
            });
        }
    }

    flows
}

// ---------------------------------------------------------------------------
// System 3: Agent UX Integration — contextual placements
// ---------------------------------------------------------------------------

fn generate_agent_placements(intent: &FlatIntent, domain: &str) -> Vec<AgentPlacement> {
    let mut placements = Vec::new();

    for agent in &intent.suggested_agents {
        let (trigger_text, context_hint) = match agent.agent_type.as_str() {
            "chatbot" => {
                let trigger = match domain {
                    "wine" => "Ask our Sommelier".to_string(),
                    "food" | "restaurant" => "Get a recommendation".to_string(),
                    "travel" | "hotel" => "Plan your trip".to_string(),
                    "fitness" | "health" => "Talk to a coach".to_string(),
                    "finance" => "Get financial guidance".to_string(),
                    _ => format!("Ask {}", agent.name),
                };
                let context = match domain {
                    "wine" => "Place near wine listings with prompt: 'What pairs well with steak tonight?'",
                    "food" | "restaurant" => "Place near menu with prompt: 'What do you recommend for a date night?'",
                    "fitness" => "Place near programs with prompt: 'I want to lose 10 pounds in 2 months'",
                    _ => "Place as floating button with contextual prompt",
                };
                (trigger, context.to_string())
            }
            "recommendation" => {
                let trigger = match domain {
                    "wine" => "Find your perfect wine →".to_string(),
                    "ecommerce" | "shop" => "Get personalized picks".to_string(),
                    _ => "Get recommendations".to_string(),
                };
                ("Embed as inline section below featured items".to_string(), trigger)
            }
            "support" => {
                ("Need help?".to_string(), "Floating button, bottom-right, opens after 30s or on error".to_string())
            }
            "analytics" => {
                ("Generate insights".to_string(), "Button in dashboard header that opens sidebar analytics chat".to_string())
            }
            _ => {
                (format!("Try {}", agent.name), "Contextual inline placement".to_string())
            }
        };

        placements.push(AgentPlacement {
            agent_name: agent.name.clone(),
            placement: agent.trigger.clone(),
            trigger_text,
            context_hint,
        });
    }

    placements
}

// ---------------------------------------------------------------------------
// System 4: Micro-Delight System
// ---------------------------------------------------------------------------

fn generate_delight_config(style: &UiStyle) -> DelightConfig {
    match style {
        UiStyle::Luxurious => DelightConfig {
            entrance_animation: "animate-fade-in".into(),
            card_hover: "transition-all duration-300 hover:-translate-y-1 hover:shadow-lg".into(),
            button_hover: "transition-all duration-300 hover:shadow-[0_0_30px_rgba(var(--primary),0.2)]".into(),
            section_reveal: "animate-fade-in [animation-delay:200ms]".into(),
            hero_animation: "animate-fade-in".into(),
        },
        UiStyle::Playful => DelightConfig {
            entrance_animation: "animate-bounce-in".into(),
            card_hover: "transition-all duration-200 hover:-translate-y-2 hover:scale-[1.02] hover:shadow-xl".into(),
            button_hover: "transition-transform duration-200 hover:scale-105 active:scale-95".into(),
            section_reveal: "animate-bounce-in [animation-delay:100ms]".into(),
            hero_animation: "animate-bounce-in".into(),
        },
        UiStyle::Corporate => DelightConfig {
            entrance_animation: "animate-fade-in".into(),
            card_hover: "transition-shadow duration-200 hover:shadow-md".into(),
            button_hover: "transition-colors duration-150".into(),
            section_reveal: "animate-fade-in".into(),
            hero_animation: "animate-fade-in".into(),
        },
        UiStyle::Bold => DelightConfig {
            entrance_animation: "animate-fade-in".into(),
            card_hover: "transition-all duration-150 hover:bg-white hover:text-black".into(),
            button_hover: "transition-all duration-150 hover:bg-white hover:text-black".into(),
            section_reveal: "animate-fade-in".into(),
            hero_animation: "animate-fade-in".into(),
        },
        _ => DelightConfig {
            entrance_animation: "animate-fade-in".into(),
            card_hover: "transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md".into(),
            button_hover: "transition-opacity duration-150 hover:opacity-90".into(),
            section_reveal: "animate-fade-in".into(),
            hero_animation: "animate-fade-in".into(),
        },
    }
}

// ---------------------------------------------------------------------------
// System 5: Hero Clarity Enforcement
// ---------------------------------------------------------------------------

fn generate_hero(domain: &str, intent: &FlatIntent, description: &str) -> HeroContent {
    let lower = description.to_lowercase();

    match domain {
        "wine" => HeroContent {
            headline: "Discover Wines You'll Love".into(),
            subheadline: "Curated selections from world-class vineyards, guided by AI. From everyday bottles to rare vintages — find your perfect pour.".into(),
            cta_text: "Explore the Collection".into(),
            cta_href: "/browse".into(),
            secondary_cta: Some(("Ask the Sommelier".into(), "#sommelier".into())),
            badges: vec!["500+ Wines".into(), "42 Regions".into(), "AI Pairing".into()],
        },
        "food" | "restaurant" => HeroContent {
            headline: "Where Every Dish Tells a Story".into(),
            subheadline: "Seasonal ingredients, bold flavors, and unforgettable experiences. Reserve your table or explore our menu.".into(),
            cta_text: "View Our Menu".into(),
            cta_href: "/menu".into(),
            secondary_cta: Some(("Reserve a Table".into(), "/reservations".into())),
            badges: vec!["Michelin Recognized".into(), "Farm to Table".into()],
        },
        "fitness" | "health" => HeroContent {
            headline: "Your Strongest Self Starts Here".into(),
            subheadline: "Personalized training programs, expert coaching, and AI-powered progress tracking. Join thousands who've transformed their health.".into(),
            cta_text: "Start Free Trial".into(),
            cta_href: "/register".into(),
            secondary_cta: Some(("View Programs".into(), "/programs".into())),
            badges: vec!["10,000+ Members".into(), "AI Coach".into(), "4.9★ Rating".into()],
        },
        "saas" | "platform" | "tool" => {
            let product_verb = if lower.contains("manage") { "Manage" }
                else if lower.contains("track") { "Track" }
                else if lower.contains("automate") { "Automate" }
                else if lower.contains("build") { "Build" }
                else if lower.contains("monitor") { "Monitor" }
                else { "Run" };

            HeroContent {
                headline: format!("{} Your Entire Business in One Place", product_verb),
                subheadline: "The all-in-one platform that replaces spreadsheets, disconnected tools, and manual processes. Start in minutes, scale forever.".into(),
                cta_text: "Get Started Free".into(),
                cta_href: "/register".into(),
                secondary_cta: Some(("See How It Works".into(), "/#features".into())),
                badges: vec!["Free Tier".into(), "No Credit Card".into(), "Setup in 2 min".into()],
            }
        }
        "marketplace" => HeroContent {
            headline: "Find Exactly What You're Looking For".into(),
            subheadline: "Browse thousands of listings from verified hosts. Book with confidence, backed by our quality guarantee.".into(),
            cta_text: "Start Browsing".into(),
            cta_href: "/browse".into(),
            secondary_cta: Some(("Become a Host".into(), "/register".into())),
            badges: vec!["1,200+ Listings".into(), "Verified Hosts".into(), "Instant Book".into()],
        },
        "ecommerce" | "shop" | "store" => HeroContent {
            headline: "Designed for the Way You Live".into(),
            subheadline: "Thoughtfully crafted products that blend form and function. Free shipping on orders over $75.".into(),
            cta_text: "Shop Now".into(),
            cta_href: "/products".into(),
            secondary_cta: Some(("New Arrivals".into(), "/products?filter=new".into())),
            badges: vec!["Free Shipping".into(), "30-Day Returns".into(), "Sustainably Made".into()],
        },
        "travel" | "hotel" => HeroContent {
            headline: "Your Next Adventure Awaits".into(),
            subheadline: "Handpicked destinations, local experiences, and AI-powered itinerary planning. Travel smarter, not harder.".into(),
            cta_text: "Plan Your Trip".into(),
            cta_href: "/explore".into(),
            secondary_cta: Some(("Talk to Concierge".into(), "#concierge".into())),
            badges: vec!["200+ Destinations".into(), "AI Concierge".into(), "Best Price Guarantee".into()],
        },
        "education" => HeroContent {
            headline: "Learn Without Limits".into(),
            subheadline: "Interactive courses, expert instructors, and AI tutoring that adapts to your pace. Start learning today.".into(),
            cta_text: "Browse Courses".into(),
            cta_href: "/courses".into(),
            secondary_cta: Some(("Free Trial".into(), "/register".into())),
            badges: vec!["500+ Courses".into(), "AI Tutor".into(), "Certificate Included".into()],
        },
        "finance" | "crypto" => HeroContent {
            headline: "Smart Money Starts Here".into(),
            subheadline: "Track your portfolio, analyze markets, and get AI-powered insights — all in one secure dashboard.".into(),
            cta_text: "Create Free Account".into(),
            cta_href: "/register".into(),
            secondary_cta: None,
            badges: vec!["Bank-grade Security".into(), "Real-time Data".into(), "AI Insights".into()],
        },
        _ => {
            // Extract a verb from the description for a dynamic headline
            let headline = if lower.contains("landing") {
                "Welcome to the Future".to_string()
            } else if lower.contains("portfolio") {
                "Crafting Digital Experiences".to_string()
            } else if lower.contains("blog") {
                "Stories Worth Reading".to_string()
            } else {
                "Built for What Matters".to_string()
            };
            HeroContent {
                headline,
                subheadline: "A modern platform designed to help you achieve more with less effort. Get started in seconds.".into(),
                cta_text: "Get Started".into(),
                cta_href: if intent.needs_auth { "/register" } else { "/#features" }.into(),
                secondary_cta: Some(("Learn More".into(), "/#features".into())),
                badges: vec![],
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Navigation generation
// ---------------------------------------------------------------------------

fn generate_nav(intent: &FlatIntent, _domain: &str) -> Vec<NavItem> {
    let mut items = Vec::new();

    // Home is always first (but not in nav — it's the logo link)
    match intent.app_type {
        AppType::ECommerce => {
            items.push(NavItem { label: "Products".into(), href: "/products".into(), is_cta: false });
            if intent.needs_payments { items.push(NavItem { label: "Cart".into(), href: "/cart".into(), is_cta: false }); }
        }
        AppType::Marketplace => {
            items.push(NavItem { label: "Browse".into(), href: "/browse".into(), is_cta: false });
        }
        AppType::SaasApp | AppType::Dashboard => {
            items.push(NavItem { label: "Dashboard".into(), href: "/dashboard".into(), is_cta: false });
            if intent.needs_payments { items.push(NavItem { label: "Pricing".into(), href: "/pricing".into(), is_cta: false }); }
        }
        AppType::Blog => {
            items.push(NavItem { label: "Articles".into(), href: "/articles".into(), is_cta: false });
            items.push(NavItem { label: "About".into(), href: "/about".into(), is_cta: false });
        }
        _ => {}
    }

    // CTA nav item
    if intent.needs_auth {
        items.push(NavItem { label: "Sign Up".into(), href: "/register".into(), is_cta: true });
    }

    items
}

// ---------------------------------------------------------------------------
// Trust signals
// ---------------------------------------------------------------------------

fn generate_trust_signals(domain: &str) -> Vec<String> {
    match domain {
        "wine" => vec!["Sourced from 42 wine regions".into(), "Expert-curated collections".into(), "Temperature-controlled shipping".into()],
        "saas" | "platform" | "tool" => vec!["Trusted by 2,000+ teams".into(), "99.9% uptime SLA".into(), "SOC 2 Type II compliant".into()],
        "ecommerce" | "shop" | "store" => vec!["Free shipping over $75".into(), "30-day hassle-free returns".into(), "Sustainably sourced materials".into()],
        "marketplace" => vec!["All hosts verified".into(), "Secure payments".into(), "24/7 support".into()],
        "fitness" | "health" => vec!["Certified trainers".into(), "Money-back guarantee".into(), "Results in 30 days".into()],
        "restaurant" | "food" => vec!["Locally sourced ingredients".into(), "Award-winning chef".into(), "Private dining available".into()],
        _ => vec!["Trusted by thousands".into(), "Enterprise-grade security".into(), "World-class support".into()],
    }
}

// ---------------------------------------------------------------------------
// Domain detection (reuses intent but adds depth)
// ---------------------------------------------------------------------------

fn detect_domain_from_intent(intent: &FlatIntent, description: &str) -> String {
    let lower = description.to_lowercase();

    // Specific domain keywords
    let domains = [
        ("wine", "wine"), ("food", "food"), ("restaurant", "restaurant"),
        ("travel", "travel"), ("hotel", "hotel"), ("fitness", "fitness"),
        ("health", "health"), ("education", "education"), ("finance", "finance"),
        ("crypto", "crypto"),
    ];
    for (keyword, domain) in &domains {
        if lower.contains(keyword) { return domain.to_string(); }
    }

    // Fall back to app type
    match intent.app_type {
        AppType::ECommerce => "ecommerce".into(),
        AppType::SaasApp => "saas".into(),
        AppType::Marketplace => "marketplace".into(),
        AppType::Dashboard => "saas".into(),
        AppType::Blog => "blog".into(),
        AppType::Portfolio => "portfolio".into(),
        _ => "general".into(),
    }
}

// ---------------------------------------------------------------------------
// Full Product Brief — personas, monetization, onboarding, retention
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullProductBrief {
    pub base: ProductBrief,
    pub personas: Vec<UserPersona>,
    pub monetization: MonetizationStrategy,
    pub onboarding: OnboardingFlow,
    pub retention: RetentionLoop,
    pub feature_priorities: Vec<FeaturePriority>,
    pub landing_page_copy: LandingPageCopy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPersona {
    pub name: String,
    pub role: String,
    pub age_range: String,
    pub goals: Vec<String>,
    pub frustrations: Vec<String>,
    pub preferred_channels: Vec<String>,
    pub willingness_to_pay: String,
    pub primary_use_case: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetizationStrategy {
    pub model: String,
    pub free_tier: Option<String>,
    pub paid_tiers: Vec<PricingTier>,
    pub upsell_triggers: Vec<String>,
    pub payment_processor: String,
    pub revenue_levers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub name: String,
    pub price: String,
    pub period: String,
    pub features: Vec<String>,
    pub is_recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingFlow {
    pub steps: Vec<OnboardingStep>,
    pub completion_incentive: String,
    pub time_to_value: String,
    pub empty_state_cta: String,
    pub checklist_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStep {
    pub step: u32,
    pub title: String,
    pub description: String,
    pub action: String,
    pub route: String,
    pub is_skippable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionLoop {
    pub daily_hook: String,
    pub weekly_hook: String,
    pub reengagement_trigger: String,
    pub streak_mechanic: Option<String>,
    pub social_proof_loop: Option<String>,
    pub notification_cadence: String,
    pub habit_forming_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePriority {
    pub feature: String,
    pub impact: String,
    pub effort: String,
    pub priority_score: u32,
    pub category: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandingPageCopy {
    pub headline: String,
    pub subheadline: String,
    pub value_props: Vec<ValueProp>,
    pub social_proof: SocialProofBlock,
    pub faq_items: Vec<FaqItem>,
    pub footer_cta: String,
    pub seo_description: String,
    pub og_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueProp {
    pub icon: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialProofBlock {
    pub stat_1: (String, String),
    pub stat_2: (String, String),
    pub stat_3: (String, String),
    pub testimonials: Vec<Testimonial>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Testimonial {
    pub quote: String,
    pub name: String,
    pub role: String,
    pub company: String,
    pub avatar_emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaqItem {
    pub question: String,
    pub answer: String,
}

/// Generate a full product brief — adds personas, monetization, onboarding, retention.
pub fn generate_full_product_brief(intent: &crate::intent_engine::FlatIntent, description: &str) -> FullProductBrief {
    let base = generate_product_brief(intent, description);
    let domain = base.domain.clone();

    FullProductBrief {
        personas: generate_personas(&domain, intent),
        monetization: generate_monetization(&domain, intent),
        onboarding: generate_onboarding(&domain, intent),
        retention: generate_retention(&domain, intent),
        feature_priorities: generate_feature_priorities(&domain, intent),
        landing_page_copy: generate_landing_page_copy(&domain, intent, description, &base),
        base,
    }
}

fn generate_personas(domain: &str, intent: &crate::intent_engine::FlatIntent) -> Vec<UserPersona> {
    match domain {
        "saas" | "platform" | "tool" => vec![
            UserPersona {
                name: "Alex Chen".into(),
                role: "Startup Founder".into(),
                age_range: "28–40".into(),
                goals: vec!["Ship product fast".into(), "Reduce operational overhead".into(), "Scale without hiring".into()],
                frustrations: vec!["Too many disconnected tools".into(), "Context switching kills productivity".into(), "Can't afford enterprise software".into()],
                preferred_channels: vec!["Product Hunt".into(), "Twitter/X".into(), "Indie Hackers".into()],
                willingness_to_pay: "$29–$99/mo".into(),
                primary_use_case: "Replace 3–4 fragmented tools with one unified platform".into(),
            },
            UserPersona {
                name: "Maria Santos".into(),
                role: "Operations Manager".into(),
                age_range: "32–50".into(),
                goals: vec!["Improve team efficiency".into(), "Better reporting for stakeholders".into(), "Reduce manual data entry".into()],
                frustrations: vec!["Spreadsheets breaking".into(), "No single source of truth".into(), "Hard to onboard new team members".into()],
                preferred_channels: vec!["LinkedIn".into(), "G2".into(), "Slack communities".into()],
                willingness_to_pay: "$49–$199/mo per team".into(),
                primary_use_case: "Centralize team workflows and automate status reporting".into(),
            },
        ],
        "ecommerce" | "shop" | "store" => vec![
            UserPersona {
                name: "Jamie Lee".into(),
                role: "Online Shopper".into(),
                age_range: "22–38".into(),
                goals: vec!["Find quality products quickly".into(), "Good value for money".into(), "Easy returns if needed".into()],
                frustrations: vec!["Misleading product photos".into(), "Slow shipping".into(), "Complicated checkout".into()],
                preferred_channels: vec!["Instagram".into(), "TikTok".into(), "Email newsletters".into()],
                willingness_to_pay: "$30–$150 per purchase".into(),
                primary_use_case: "Discover and purchase curated products with confidence".into(),
            },
        ],
        "marketplace" => vec![
            UserPersona {
                name: "Taylor Kim".into(),
                role: "Marketplace Buyer".into(),
                age_range: "25–45".into(),
                goals: vec!["Find verified providers".into(), "Compare options easily".into(), "Secure booking process".into()],
                frustrations: vec!["Fake listings".into(), "Hidden fees".into(), "Slow responses from sellers".into()],
                preferred_channels: vec!["Google Search".into(), "Word of mouth".into(), "App stores".into()],
                willingness_to_pay: "Variable based on listing".into(),
                primary_use_case: "Book or purchase with full confidence in quality and safety".into(),
            },
            UserPersona {
                name: "Sam Rivera".into(),
                role: "Marketplace Seller".into(),
                age_range: "25–55".into(),
                goals: vec!["Reach more customers".into(), "Manage listings easily".into(), "Get paid reliably".into()],
                frustrations: vec!["High platform fees".into(), "Poor analytics".into(), "Slow dispute resolution".into()],
                preferred_channels: vec!["Facebook Groups".into(), "Industry forums".into(), "Direct outreach".into()],
                willingness_to_pay: "5–15% commission".into(),
                primary_use_case: "List services/products and grow revenue with minimal overhead".into(),
            },
        ],
        "fitness" | "health" => vec![
            UserPersona {
                name: "Jordan Park".into(),
                role: "Health-Conscious Professional".into(),
                age_range: "25–45".into(),
                goals: vec!["Build sustainable fitness habits".into(), "See measurable progress".into(), "Fit workouts into busy schedule".into()],
                frustrations: vec!["Programs too generic".into(), "Inconsistent motivation".into(), "Hard to track progress".into()],
                preferred_channels: vec!["YouTube".into(), "Reddit fitness subs".into(), "App stores".into()],
                willingness_to_pay: "$15–$50/mo".into(),
                primary_use_case: "Follow a structured program and track real progress over time".into(),
            },
        ],
        _ => {
            let app_type = format!("{:?}", intent.app_type).to_lowercase();
            vec![
                UserPersona {
                    name: "Primary User".into(),
                    role: format!("{} user", app_type),
                    age_range: "25–45".into(),
                    goals: vec!["Solve core problem efficiently".into(), "Save time".into(), "Achieve measurable results".into()],
                    frustrations: vec!["Existing solutions are too complex".into(), "Poor UX in current tools".into(), "Too expensive for the value".into()],
                    preferred_channels: vec!["Google".into(), "Word of mouth".into(), "Social media".into()],
                    willingness_to_pay: "$10–$50/mo".into(),
                    primary_use_case: "Get the job done faster and better than current alternatives".into(),
                },
            ]
        }
    }
}

fn generate_monetization(domain: &str, intent: &crate::intent_engine::FlatIntent) -> MonetizationStrategy {
    if !intent.needs_payments {
        return MonetizationStrategy {
            model: "freemium".into(),
            free_tier: Some("Full access with usage limits".into()),
            paid_tiers: vec![
                PricingTier {
                    name: "Pro".into(),
                    price: "$19".into(),
                    period: "month".into(),
                    features: vec!["Unlimited usage".into(), "Priority support".into(), "Advanced features".into()],
                    is_recommended: true,
                },
            ],
            upsell_triggers: vec!["Usage limit hit".into(), "Team invite attempt".into(), "Export request".into()],
            payment_processor: "Stripe".into(),
            revenue_levers: vec!["Annual discount (2 months free)".into(), "Team pricing multiplier".into()],
        };
    }

    match domain {
        "saas" | "platform" | "tool" => MonetizationStrategy {
            model: "freemium + per-seat SaaS".into(),
            free_tier: Some("Solo plan: 1 user, 1,000 records, community support".into()),
            paid_tiers: vec![
                PricingTier {
                    name: "Starter".into(),
                    price: "$0".into(),
                    period: "month".into(),
                    features: vec!["1 user".into(), "1,000 records".into(), "Community support".into()],
                    is_recommended: false,
                },
                PricingTier {
                    name: "Pro".into(),
                    price: "$29".into(),
                    period: "month".into(),
                    features: vec!["5 users".into(), "Unlimited records".into(), "API access".into(), "Priority support".into()],
                    is_recommended: true,
                },
                PricingTier {
                    name: "Team".into(),
                    price: "$79".into(),
                    period: "month".into(),
                    features: vec!["Unlimited users".into(), "SSO".into(), "Audit logs".into(), "Dedicated support".into()],
                    is_recommended: false,
                },
            ],
            upsell_triggers: vec![
                "Record limit approaching (80%)".into(),
                "User invite on free plan".into(),
                "API call attempt on free plan".into(),
                "Export to CSV attempt".into(),
            ],
            payment_processor: "Stripe".into(),
            revenue_levers: vec![
                "Annual billing discount (20%)".into(),
                "Per-seat pricing above 5 users".into(),
                "Usage-based overage for records".into(),
                "Add-on: white-label ($99/mo)".into(),
            ],
        },
        "ecommerce" | "shop" | "store" => MonetizationStrategy {
            model: "direct e-commerce".into(),
            free_tier: None,
            paid_tiers: vec![],
            upsell_triggers: vec![
                "Cart abandonment email at 1h".into(),
                "Post-purchase upsell (related items)".into(),
                "Bundle discount at checkout".into(),
                "Loyalty points milestone".into(),
            ],
            payment_processor: "Stripe".into(),
            revenue_levers: vec![
                "AOV increase: bundle deals".into(),
                "Subscription: replenishment items".into(),
                "Loyalty program: repeat purchase incentive".into(),
                "Upsell: warranty / extended service".into(),
            ],
        },
        "marketplace" => MonetizationStrategy {
            model: "transaction fees + seller subscriptions".into(),
            free_tier: Some("Buyers always free; sellers get 3 free listings".into()),
            paid_tiers: vec![
                PricingTier {
                    name: "Seller Basic".into(),
                    price: "$0".into(),
                    period: "month".into(),
                    features: vec!["3 listings".into(), "5% transaction fee".into()],
                    is_recommended: false,
                },
                PricingTier {
                    name: "Seller Pro".into(),
                    price: "$29".into(),
                    period: "month".into(),
                    features: vec!["Unlimited listings".into(), "3% transaction fee".into(), "Featured placement".into()],
                    is_recommended: true,
                },
            ],
            upsell_triggers: vec!["3rd listing attempt".into(), "High search visibility request".into()],
            payment_processor: "Stripe Connect".into(),
            revenue_levers: vec!["Transaction fees".into(), "Featured listing boost".into(), "Seller analytics premium".into()],
        },
        _ => MonetizationStrategy {
            model: "freemium".into(),
            free_tier: Some("Core features free".into()),
            paid_tiers: vec![
                PricingTier {
                    name: "Pro".into(),
                    price: "$19".into(),
                    period: "month".into(),
                    features: vec!["All features".into(), "Priority support".into()],
                    is_recommended: true,
                },
            ],
            upsell_triggers: vec!["Feature gate hit".into(), "Usage limit reached".into()],
            payment_processor: "Stripe".into(),
            revenue_levers: vec!["Annual billing".into(), "Team expansion".into()],
        },
    }
}

fn generate_onboarding(domain: &str, intent: &crate::intent_engine::FlatIntent) -> OnboardingFlow {
    let base_steps = if intent.needs_auth {
        vec![
            OnboardingStep {
                step: 1,
                title: "Create your account".into(),
                description: "Sign up with email or Google. No credit card required.".into(),
                action: "Sign up".into(),
                route: "/register".into(),
                is_skippable: false,
            },
        ]
    } else {
        vec![]
    };

    let domain_steps: Vec<OnboardingStep> = match domain {
        "saas" | "platform" | "tool" => vec![
            OnboardingStep {
                step: (base_steps.len() + 1) as u32,
                title: "Tell us about your team".into(),
                description: "Help us personalize your experience.".into(),
                action: "Continue".into(),
                route: "/onboarding/team".into(),
                is_skippable: true,
            },
            OnboardingStep {
                step: (base_steps.len() + 2) as u32,
                title: "Create your first item".into(),
                description: "Get hands-on with the core feature in under 2 minutes.".into(),
                action: "Create".into(),
                route: "/dashboard/new".into(),
                is_skippable: false,
            },
            OnboardingStep {
                step: (base_steps.len() + 3) as u32,
                title: "Invite your team".into(),
                description: "Collaborate is better together. Invite up to 3 teammates free.".into(),
                action: "Invite teammates".into(),
                route: "/settings/team".into(),
                is_skippable: true,
            },
        ],
        "ecommerce" | "shop" | "store" => vec![
            OnboardingStep {
                step: 1,
                title: "Browse our collection".into(),
                description: "Explore hundreds of curated products.".into(),
                action: "Browse now".into(),
                route: "/products".into(),
                is_skippable: false,
            },
            OnboardingStep {
                step: 2,
                title: "Save favorites".into(),
                description: "Heart items to build your wishlist.".into(),
                action: "Save item".into(),
                route: "/products".into(),
                is_skippable: true,
            },
        ],
        "fitness" | "health" => vec![
            OnboardingStep {
                step: (base_steps.len() + 1) as u32,
                title: "Set your goal".into(),
                description: "What do you want to achieve? We'll build a personalized plan.".into(),
                action: "Set goal".into(),
                route: "/onboarding/goal".into(),
                is_skippable: false,
            },
            OnboardingStep {
                step: (base_steps.len() + 2) as u32,
                title: "Choose your first workout".into(),
                description: "Pick from programs designed for your goal and fitness level.".into(),
                action: "Pick program".into(),
                route: "/programs".into(),
                is_skippable: false,
            },
        ],
        _ => vec![
            OnboardingStep {
                step: (base_steps.len() + 1) as u32,
                title: "Explore the dashboard".into(),
                description: "Take a 60-second tour of the main features.".into(),
                action: "Start tour".into(),
                route: "/dashboard".into(),
                is_skippable: true,
            },
        ],
    };

    let mut all_steps = base_steps;
    all_steps.extend(domain_steps);

    let completion_incentive = match domain {
        "saas" | "platform" | "tool" => "Complete onboarding to unlock 14-day Pro trial".into(),
        "fitness" | "health" => "Complete setup to get your personalized 4-week plan".into(),
        "ecommerce" | "shop" | "store" => "Complete your profile to get 10% off your first order".into(),
        _ => "Complete setup to unlock all features".into(),
    };

    OnboardingFlow {
        steps: all_steps,
        completion_incentive,
        time_to_value: "Under 3 minutes".into(),
        empty_state_cta: format!("Create your first {} to get started", match domain {
            "saas" | "platform" | "tool" => "project",
            "ecommerce" | "shop" | "store" => "wishlist",
            "marketplace" => "listing",
            "fitness" | "health" => "workout",
            _ => "item",
        }),
        checklist_items: vec![
            "Account created".into(),
            "Profile complete".into(),
            "First action taken".into(),
            "Core feature explored".into(),
        ],
    }
}

fn generate_retention(domain: &str, intent: &crate::intent_engine::FlatIntent) -> RetentionLoop {
    let _ = intent;

    match domain {
        "saas" | "platform" | "tool" => RetentionLoop {
            daily_hook: "Daily digest email: '3 things changed in your workspace'".into(),
            weekly_hook: "Weekly summary: usage stats, team activity, top insights".into(),
            reengagement_trigger: "Send 'We miss you' after 7 days inactive with top feature highlight".into(),
            streak_mechanic: None,
            social_proof_loop: Some("Show '143 teams used this feature this week' on key screens".into()),
            notification_cadence: "Day 1: welcome. Day 3: pro tip. Day 7: weekly report. Day 14: upgrade nudge.".into(),
            habit_forming_features: vec![
                "Dashboard as homepage — daily reason to return".into(),
                "Notifications for team activity".into(),
                "Weekly email reports (opt-out, not opt-in)".into(),
                "Saved views / pinned items".into(),
            ],
        },
        "fitness" | "health" => RetentionLoop {
            daily_hook: "Daily workout reminder at user's preferred time with personalized message".into(),
            weekly_hook: "Weekly progress report: workouts completed, calories, personal records".into(),
            reengagement_trigger: "'Your streak is at risk' push notification after 2 days missed".into(),
            streak_mechanic: Some("Daily workout streak with milestone rewards at 7, 30, 100 days".into()),
            social_proof_loop: Some("Leaderboard: 'You're in the top 23% this week'".into()),
            notification_cadence: "Daily workout reminder + weekly progress + monthly goal check-in".into(),
            habit_forming_features: vec![
                "Streak counter on dashboard".into(),
                "Progress photos / measurements tracker".into(),
                "Community challenges".into(),
                "AI coach check-ins".into(),
            ],
        },
        "ecommerce" | "shop" | "store" => RetentionLoop {
            daily_hook: "Flash deal or new arrival notification (max 3x/week to avoid fatigue)".into(),
            weekly_hook: "Curated picks email: 'Based on what you love'".into(),
            reengagement_trigger: "Wishlist price drop notification".into(),
            streak_mechanic: None,
            social_proof_loop: Some("'12 people are viewing this item right now'".into()),
            notification_cadence: "Post-purchase: day 1 shipping update, day 5 delivery confirm, day 10 review request".into(),
            habit_forming_features: vec![
                "Wishlist / saved items".into(),
                "Loyalty points".into(),
                "Replenishment reminders".into(),
                "Early access for repeat buyers".into(),
            ],
        },
        "marketplace" => RetentionLoop {
            daily_hook: "New listings matching your saved searches".into(),
            weekly_hook: "Price drops on saved listings".into(),
            reengagement_trigger: "'A new listing matches your criteria' after 14 days no visit".into(),
            streak_mechanic: None,
            social_proof_loop: Some("'12 bookings made this week in your area'".into()),
            notification_cadence: "Saved search alerts (real-time), weekly recommendations".into(),
            habit_forming_features: vec![
                "Saved searches with alerts".into(),
                "Favorite sellers / hosts".into(),
                "Review request after booking".into(),
                "Loyalty perks for repeat bookings".into(),
            ],
        },
        _ => RetentionLoop {
            daily_hook: "Daily activity summary or personalized tip".into(),
            weekly_hook: "Weekly progress and usage report".into(),
            reengagement_trigger: "Re-engagement email after 7 days with value highlight".into(),
            streak_mechanic: None,
            social_proof_loop: None,
            notification_cadence: "Welcome series (days 1, 3, 7) then weekly digest".into(),
            habit_forming_features: vec![
                "Dashboard with quick actions".into(),
                "Progress tracking".into(),
                "Notifications for key events".into(),
            ],
        },
    }
}

fn generate_feature_priorities(domain: &str, intent: &crate::intent_engine::FlatIntent) -> Vec<FeaturePriority> {
    let mut features = Vec::new();

    // Auth-related features
    if intent.needs_auth {
        features.push(FeaturePriority {
            feature: "User authentication (sign up / login)".into(),
            impact: "critical".into(),
            effort: "low".into(),
            priority_score: 95,
            category: "foundation".into(),
            rationale: "No other feature works without auth. Use NextAuth or Clerk for speed.".into(),
        });
    }

    // Database features
    if intent.needs_database {
        features.push(FeaturePriority {
            feature: "Data persistence layer".into(),
            impact: "critical".into(),
            effort: "medium".into(),
            priority_score: 90,
            category: "foundation".into(),
            rationale: "Core data must be persisted before any feature can be validated.".into(),
        });
    }

    // Domain-specific high-value features
    match domain {
        "saas" | "platform" | "tool" => {
            features.extend([
                FeaturePriority {
                    feature: "Core workflow (the main thing the app does)".into(),
                    impact: "critical".into(), effort: "high".into(), priority_score: 88,
                    category: "core".into(),
                    rationale: "This is why users signed up. Ship it before anything else.".into(),
                },
                FeaturePriority {
                    feature: "Dashboard with key metrics".into(),
                    impact: "high".into(), effort: "medium".into(), priority_score: 75,
                    category: "retention".into(),
                    rationale: "Gives users a reason to return daily. Drives activation.".into(),
                },
                FeaturePriority {
                    feature: "CSV / data export".into(),
                    impact: "medium".into(), effort: "low".into(), priority_score: 65,
                    category: "growth".into(),
                    rationale: "Common request, easy to build. Good for word-of-mouth.".into(),
                },
                FeaturePriority {
                    feature: "Team invite / collaboration".into(),
                    impact: "high".into(), effort: "medium".into(), priority_score: 70,
                    category: "growth".into(),
                    rationale: "Viral loop: each user brings teammates. Grows MRR per account.".into(),
                },
            ]);
        }
        "ecommerce" | "shop" | "store" => {
            features.extend([
                FeaturePriority {
                    feature: "Product catalog with search & filter".into(),
                    impact: "critical".into(), effort: "medium".into(), priority_score: 92,
                    category: "core".into(), rationale: "Browsability is table stakes for e-commerce.".into(),
                },
                FeaturePriority {
                    feature: "Cart & checkout with Stripe".into(),
                    impact: "critical".into(), effort: "medium".into(), priority_score: 90,
                    category: "core".into(), rationale: "No cart = no revenue. Ship fast.".into(),
                },
                FeaturePriority {
                    feature: "Order history & tracking".into(),
                    impact: "high".into(), effort: "low".into(), priority_score: 72,
                    category: "retention".into(), rationale: "Reduces support tickets, improves satisfaction.".into(),
                },
            ]);
        }
        _ => {
            features.push(FeaturePriority {
                feature: "Core feature (primary value proposition)".into(),
                impact: "critical".into(), effort: "high".into(), priority_score: 95,
                category: "core".into(), rationale: "Ship what users signed up for first.".into(),
            });
        }
    }

    // Sort by priority score descending
    features.sort_by(|a, b| b.priority_score.cmp(&a.priority_score));
    features
}

fn generate_landing_page_copy(
    domain: &str,
    intent: &crate::intent_engine::FlatIntent,
    description: &str,
    base: &ProductBrief,
) -> LandingPageCopy {
    let _ = description;

    let value_props = match domain {
        "saas" | "platform" | "tool" => vec![
            ValueProp { icon: "⚡".into(), title: "Built for speed".into(), description: "Set up in minutes, not weeks. Import your data and you're live.".into() },
            ValueProp { icon: "🔒".into(), title: "Enterprise-grade security".into(), description: "SOC 2 compliant, end-to-end encryption, GDPR ready.".into() },
            ValueProp { icon: "📊".into(), title: "Real-time insights".into(), description: "Dashboards that update live. No more stale spreadsheets.".into() },
            ValueProp { icon: "🤝".into(), title: "Built for teams".into(), description: "Role-based access, shared workspaces, and activity feeds.".into() },
        ],
        "ecommerce" | "shop" | "store" => vec![
            ValueProp { icon: "🚚".into(), title: "Free shipping".into(), description: "On orders over $75. Delivered in 2–5 business days.".into() },
            ValueProp { icon: "↩️".into(), title: "30-day returns".into(), description: "Not happy? Return it hassle-free, no questions asked.".into() },
            ValueProp { icon: "🌱".into(), title: "Sustainably made".into(), description: "Ethically sourced materials and carbon-neutral shipping.".into() },
            ValueProp { icon: "⭐".into(), title: "5-star rated".into(), description: "Thousands of happy customers. 4.9/5 average rating.".into() },
        ],
        "fitness" | "health" => vec![
            ValueProp { icon: "🏆".into(), title: "Certified trainers".into(), description: "Every program designed by certified fitness professionals.".into() },
            ValueProp { icon: "📈".into(), title: "Track real progress".into(), description: "See your gains over time with built-in metrics and milestones.".into() },
            ValueProp { icon: "🤖".into(), title: "AI-personalized".into(), description: "Your plan adapts to your progress automatically.".into() },
            ValueProp { icon: "💪".into(), title: "Proven results".into(), description: "93% of members report visible results within 30 days.".into() },
        ],
        _ => vec![
            ValueProp { icon: "⚡".into(), title: "Fast".into(), description: "Get up and running in minutes, not months.".into() },
            ValueProp { icon: "🔒".into(), title: "Secure".into(), description: "Your data is encrypted and protected at every step.".into() },
            ValueProp { icon: "📞".into(), title: "Supported".into(), description: "Real humans ready to help. Average response under 5 minutes.".into() },
        ],
    };

    let social_proof = match domain {
        "saas" | "platform" | "tool" => SocialProofBlock {
            stat_1: ("2,000+".into(), "Teams using it daily".into()),
            stat_2: ("99.9%".into(), "Uptime SLA".into()),
            stat_3: ("4.9/5".into(), "Average rating on G2".into()),
            testimonials: vec![
                Testimonial {
                    quote: "Cut our reporting time from 4 hours to 20 minutes.".into(),
                    name: "Sarah K.".into(), role: "COO".into(), company: "GrowthLabs".into(), avatar_emoji: "👩‍💼".into(),
                },
                Testimonial {
                    quote: "Finally, a tool our whole team actually uses.".into(),
                    name: "Marcus T.".into(), role: "Product Lead".into(), company: "Stackly".into(), avatar_emoji: "👨‍💻".into(),
                },
            ],
        },
        "fitness" | "health" => SocialProofBlock {
            stat_1: ("10,000+".into(), "Active members".into()),
            stat_2: ("93%".into(), "Report visible results in 30 days".into()),
            stat_3: ("4.9★".into(), "Average app rating".into()),
            testimonials: vec![
                Testimonial {
                    quote: "Lost 18 lbs in 3 months. The AI coach kept me accountable.".into(),
                    name: "Lisa M.".into(), role: "Member since 2024".into(), company: "".into(), avatar_emoji: "🏃‍♀️".into(),
                },
            ],
        },
        _ => SocialProofBlock {
            stat_1: ("1,000+".into(), "Happy users".into()),
            stat_2: ("4.8/5".into(), "Customer satisfaction".into()),
            stat_3: ("99%".into(), "Would recommend".into()),
            testimonials: vec![
                Testimonial {
                    quote: "Exactly what we were looking for. Simple, fast, and it works.".into(),
                    name: "Alex R.".into(), role: "Founder".into(), company: "".into(), avatar_emoji: "👤".into(),
                },
            ],
        },
    };

    let faq_items = match domain {
        "saas" | "platform" | "tool" => vec![
            FaqItem { question: "Is there a free plan?".into(), answer: "Yes! Our Starter plan is free forever with core features. No credit card needed.".into() },
            FaqItem { question: "Can I import my existing data?".into(), answer: "Absolutely. We support CSV import and have direct integrations with the most popular tools.".into() },
            FaqItem { question: "How do I cancel?".into(), answer: "Cancel any time from your billing settings. We don't lock you in or charge cancellation fees.".into() },
            FaqItem { question: "Is my data secure?".into(), answer: "Yes. All data is encrypted at rest and in transit. We're SOC 2 Type II compliant.".into() },
        ],
        _ => vec![
            FaqItem { question: "How do I get started?".into(), answer: "Click 'Get Started', create your account, and you'll be up and running in under 3 minutes.".into() },
            FaqItem { question: "Do you offer refunds?".into(), answer: "Yes, we offer a 30-day money-back guarantee, no questions asked.".into() },
            FaqItem { question: "Is support available?".into(), answer: "Yes. Email support is available 7 days a week. Pro plans include priority chat support.".into() },
        ],
    };

    let og_title = format!("{} — {}", base.hero.headline, if intent.needs_auth { "Sign up free" } else { "Learn more" });
    let seo_desc = format!("{} {}", base.hero.subheadline, base.trust_signals.first().cloned().unwrap_or_default());

    LandingPageCopy {
        headline: base.hero.headline.clone(),
        subheadline: base.hero.subheadline.clone(),
        value_props,
        social_proof,
        faq_items,
        footer_cta: format!("Ready to get started? {}", base.hero.cta_text),
        seo_description: seo_desc[..seo_desc.len().min(160)].to_string(),
        og_title,
    }
}

/// Format the full brief as LLM prompt context.
pub fn format_full_brief_for_prompt(brief: &FullProductBrief) -> String {
    let mut out = format_brief_for_prompt(&brief.base);

    // Personas
    out.push_str("\n## Target User Personas\n");
    for p in &brief.personas {
        out.push_str(&format!(
            "### {} ({})\n- Goals: {}\n- Frustrations: {}\n- WTP: {}\n",
            p.name, p.role,
            p.goals.join(", "),
            p.frustrations.join(", "),
            p.willingness_to_pay,
        ));
    }

    // Monetization
    out.push_str(&format!(
        "\n## Monetization: {}\n",
        brief.monetization.model
    ));
    for tier in &brief.monetization.paid_tiers {
        out.push_str(&format!(
            "- {} tier: {}/{} — {}\n",
            tier.name, tier.price, tier.period,
            tier.features.join(", ")
        ));
    }
    if !brief.monetization.upsell_triggers.is_empty() {
        out.push_str(&format!(
            "Upsell triggers: {}\n",
            brief.monetization.upsell_triggers.join(" | ")
        ));
    }

    // Onboarding
    out.push_str("\n## Onboarding Flow\n");
    for step in &brief.onboarding.steps {
        out.push_str(&format!(
            "Step {}: {} → {} (route: {}){}\n",
            step.step, step.title, step.action, step.route,
            if step.is_skippable { " [skippable]" } else { "" }
        ));
    }
    out.push_str(&format!("Completion incentive: {}\n", brief.onboarding.completion_incentive));

    // Retention
    out.push_str("\n## Retention Loop\n");
    out.push_str(&format!("- Daily hook: {}\n", brief.retention.daily_hook));
    out.push_str(&format!("- Weekly hook: {}\n", brief.retention.weekly_hook));
    out.push_str(&format!("- Re-engagement: {}\n", brief.retention.reengagement_trigger));
    if let Some(ref streak) = brief.retention.streak_mechanic {
        out.push_str(&format!("- Streak mechanic: {}\n", streak));
    }

    // Feature priorities
    out.push_str("\n## Feature Priorities (ship in this order)\n");
    for (i, f) in brief.feature_priorities.iter().take(5).enumerate() {
        out.push_str(&format!(
            "{}. [{}] {} — {}\n",
            i + 1, f.impact.to_uppercase(), f.feature, f.rationale
        ));
    }

    out
}

/// Format the product brief as context for the LLM spec generation prompt.
/// This gives the LLM rich, specific content to use instead of generating generic text.
pub fn format_brief_for_prompt(brief: &ProductBrief) -> String {
    let mut out = String::with_capacity(2000);

    // Hero
    out.push_str(&format!(
        "## Hero Section\n- Headline: \"{}\"\n- Subheadline: \"{}\"\n- CTA: \"{}\" → {}\n",
        brief.hero.headline, brief.hero.subheadline, brief.hero.cta_text, brief.hero.cta_href,
    ));
    if let Some((text, href)) = &brief.hero.secondary_cta {
        out.push_str(&format!("- Secondary CTA: \"{}\" → {}\n", text, href));
    }
    if !brief.hero.badges.is_empty() {
        out.push_str(&format!("- Badges: {}\n", brief.hero.badges.join(", ")));
    }

    // Seed content
    for section in &brief.seed_content {
        out.push_str(&format!("\n## {} Section\n", section.title));
        for item in &section.items {
            out.push_str(&format!("- {} {} — {}", item.image_emoji, item.name, item.description));
            if let Some(price) = &item.price {
                out.push_str(&format!(" ({})", price));
            }
            if let Some(badge) = &item.badge {
                out.push_str(&format!(" [{}]", badge));
            }
            out.push('\n');
        }
    }

    // Agent placements
    if !brief.agent_placements.is_empty() {
        out.push_str("\n## AI Agent Integration\n");
        for ap in &brief.agent_placements {
            out.push_str(&format!(
                "- {}: trigger=\"{}\" placement={} ({})\n",
                ap.agent_name, ap.trigger_text, ap.placement, ap.context_hint,
            ));
        }
    }

    // Animations
    out.push_str(&format!(
        "\n## Animations (CSS classes to apply)\n- Cards: {}\n- Buttons: {}\n- Sections: {}\n- Hero: {}\n",
        brief.delight_classes.card_hover,
        brief.delight_classes.button_hover,
        brief.delight_classes.section_reveal,
        brief.delight_classes.hero_animation,
    ));

    // Trust signals
    if !brief.trust_signals.is_empty() {
        out.push_str(&format!(
            "\n## Trust Signals (show below hero or in footer)\n{}\n",
            brief.trust_signals.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n"),
        ));
    }

    // Navigation
    out.push_str("\n## Navigation\n");
    for item in &brief.nav_items {
        out.push_str(&format!("- {} → {}{}\n", item.label, item.href,
            if item.is_cta { " [CTA button]" } else { "" }));
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine::analyze_flat;

    #[test]
    fn wine_app_gets_realistic_content() {
        let intent = analyze_flat("luxury wine marketplace with AI sommelier");
        let brief = generate_product_brief(&intent, "luxury wine marketplace with AI sommelier");

        assert_eq!(brief.domain, "wine");
        assert!(brief.hero.headline.contains("Wine"));
        assert!(!brief.hero.badges.is_empty());
        assert!(brief.seed_content.iter().any(|s| s.items.iter().any(|i| i.name.contains("Chateau"))));
        assert!(brief.agent_placements.iter().any(|a| a.trigger_text.contains("Sommelier")));
    }

    #[test]
    fn saas_app_gets_pricing_tiers() {
        let intent = analyze_flat("SaaS platform for project management");
        let brief = generate_product_brief(&intent, "SaaS platform for project management");

        assert!(brief.seed_content.iter().any(|s| s.section_id == "pricing"));
        let pricing = brief.seed_content.iter().find(|s| s.section_id == "pricing").unwrap();
        assert!(pricing.items.len() >= 2);
        assert!(pricing.items.iter().any(|i| i.price.is_some()));
    }

    #[test]
    fn hero_has_real_benefit() {
        let intent = analyze_flat("wine landing page");
        let brief = generate_product_brief(&intent, "wine landing page");

        // Hero should NOT contain generic text
        assert!(!brief.hero.headline.contains("Lorem"));
        assert!(!brief.hero.headline.contains("Welcome to"));
        assert!(!brief.hero.subheadline.is_empty());
        assert!(!brief.hero.cta_text.is_empty());
    }

    #[test]
    fn ecommerce_has_purchase_flow() {
        let intent = analyze_flat("e-commerce store for fashion");
        let brief = generate_product_brief(&intent, "e-commerce store for fashion");

        assert!(!brief.flows.is_empty());
        let flow = &brief.flows[0];
        assert!(flow.steps.iter().any(|s| s.label.to_lowercase().contains("cart") || s.label.to_lowercase().contains("checkout")));
    }

    #[test]
    fn delight_config_matches_style() {
        let intent = analyze_flat("playful kids education app");
        let brief = generate_product_brief(&intent, "playful kids education app");

        assert!(brief.delight_classes.card_hover.contains("scale"));
    }

    #[test]
    fn prompt_format_is_rich() {
        let intent = analyze_flat("wine marketplace with AI sommelier");
        let brief = generate_product_brief(&intent, "wine marketplace with AI sommelier");
        let prompt = format_brief_for_prompt(&brief);

        assert!(prompt.contains("Chateau Margaux"));
        assert!(prompt.contains("Discover Wines"));
        assert!(prompt.contains("Sommelier"));
        assert!(prompt.len() > 500); // Rich, not sparse
    }
}
