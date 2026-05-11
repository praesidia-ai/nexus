use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTemplate {
    pub name: String,
    pub description: String,
    pub template_type: TemplateType,
    pub tech_stack: TechStack,
    pub features: Vec<Feature>,
    pub files: Vec<TemplateFile>,
    pub env_vars: Vec<EnvVar>,
    pub post_install: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateType {
    Saas,
    Ecommerce,
    Marketplace,
    InternalTool,
    ApiPlatform,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechStack {
    pub frontend: String,
    pub backend: String,
    pub database: String,
    pub auth: String,
    pub deployment: String,
    pub additional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub agent_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    pub path: String,
    pub content: String,
    pub generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

impl AppTemplate {
    pub fn saas() -> Self {
        Self {
            name: "SaaS Starter".to_string(),
            description: "Full-featured SaaS application with auth, billing, dashboard, and team management".to_string(),
            template_type: TemplateType::Saas,
            tech_stack: TechStack {
                frontend: "Next.js 15 + React + Tailwind CSS + shadcn/ui".to_string(),
                backend: "Next.js API Routes + Prisma ORM".to_string(),
                database: "PostgreSQL (Supabase)".to_string(),
                auth: "NextAuth.js with OAuth + Magic Links".to_string(),
                deployment: "Vercel + Supabase".to_string(),
                additional: vec![
                    "Stripe for billing".to_string(),
                    "Resend for email".to_string(),
                ],
            },
            features: vec![
                Feature {
                    name: "Authentication".to_string(),
                    description: "OAuth, email/password, magic links".to_string(),
                    required: true,
                    agent_role: "auth".to_string(),
                },
                Feature {
                    name: "Billing".to_string(),
                    description: "Stripe subscriptions, usage tracking, invoices".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Dashboard".to_string(),
                    description: "Analytics, metrics, charts".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
                Feature {
                    name: "Team Management".to_string(),
                    description: "Invite, roles, permissions".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Settings".to_string(),
                    description: "Profile, org settings, API keys".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
            ],
            files: saas_files(),
            env_vars: vec![
                EnvVar {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL connection string".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "NEXTAUTH_SECRET".to_string(),
                    description: "Auth signing secret".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "STRIPE_SECRET_KEY".to_string(),
                    description: "Stripe secret key".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "STRIPE_WEBHOOK_SECRET".to_string(),
                    description: "Stripe webhook signing secret".to_string(),
                    required: true,
                    default: None,
                },
            ],
            post_install: vec![
                "npm install".to_string(),
                "npx prisma generate".to_string(),
                "npx prisma db push".to_string(),
            ],
        }
    }

    pub fn ecommerce() -> Self {
        Self {
            name: "E-Commerce".to_string(),
            description: "Online store with product catalog, cart, checkout, and Stripe payments"
                .to_string(),
            template_type: TemplateType::Ecommerce,
            tech_stack: TechStack {
                frontend: "Next.js 15 + React + Tailwind CSS + shadcn/ui".to_string(),
                backend: "Next.js API Routes + Prisma ORM".to_string(),
                database: "PostgreSQL".to_string(),
                auth: "NextAuth.js".to_string(),
                deployment: "Vercel".to_string(),
                additional: vec![
                    "Stripe Checkout".to_string(),
                    "Cloudinary for images".to_string(),
                ],
            },
            features: vec![
                Feature {
                    name: "Product Catalog".to_string(),
                    description: "Products, categories, search, filters".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Shopping Cart".to_string(),
                    description: "Add/remove items, persistent cart".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
                Feature {
                    name: "Checkout".to_string(),
                    description: "Stripe Checkout, order confirmation".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Admin Panel".to_string(),
                    description: "Product management, order tracking".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
            ],
            files: Vec::new(),
            env_vars: vec![
                EnvVar {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL connection string".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "STRIPE_SECRET_KEY".to_string(),
                    description: "Stripe secret key".to_string(),
                    required: true,
                    default: None,
                },
            ],
            post_install: vec!["npm install".to_string(), "npx prisma generate".to_string()],
        }
    }

    pub fn marketplace() -> Self {
        Self {
            name: "Marketplace".to_string(),
            description:
                "Two-sided marketplace with listings, messaging, and Stripe Connect payments"
                    .to_string(),
            template_type: TemplateType::Marketplace,
            tech_stack: TechStack {
                frontend: "Next.js 15 + React + Tailwind CSS".to_string(),
                backend: "Next.js API Routes + Prisma ORM".to_string(),
                database: "PostgreSQL".to_string(),
                auth: "NextAuth.js with multi-role".to_string(),
                deployment: "Vercel".to_string(),
                additional: vec![
                    "Stripe Connect".to_string(),
                    "Pusher for messaging".to_string(),
                ],
            },
            features: vec![
                Feature {
                    name: "Listings".to_string(),
                    description: "Create, browse, search, filter listings".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Messaging".to_string(),
                    description: "Real-time chat between buyers/sellers".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Payments".to_string(),
                    description: "Stripe Connect for split payments".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Reviews".to_string(),
                    description: "Rating and review system".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
            ],
            files: Vec::new(),
            env_vars: vec![
                EnvVar {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "STRIPE_SECRET_KEY".to_string(),
                    description: "Stripe".to_string(),
                    required: true,
                    default: None,
                },
            ],
            post_install: vec!["npm install".to_string()],
        }
    }

    pub fn internal_tool() -> Self {
        Self {
            name: "Internal Tool".to_string(),
            description: "Admin panel with CRUD, roles, audit logging, and reporting".to_string(),
            template_type: TemplateType::InternalTool,
            tech_stack: TechStack {
                frontend: "Next.js 15 + React + Tailwind CSS + shadcn/ui".to_string(),
                backend: "Next.js API Routes + Prisma".to_string(),
                database: "PostgreSQL".to_string(),
                auth: "NextAuth.js with RBAC".to_string(),
                deployment: "Docker + self-hosted".to_string(),
                additional: vec!["Audit logging".to_string()],
            },
            features: vec![
                Feature {
                    name: "CRUD".to_string(),
                    description: "Auto-generated CRUD for entities".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "RBAC".to_string(),
                    description: "Role-based access control".to_string(),
                    required: true,
                    agent_role: "auth".to_string(),
                },
                Feature {
                    name: "Audit Log".to_string(),
                    description: "Track all changes".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Reports".to_string(),
                    description: "Data export, charts".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
            ],
            files: Vec::new(),
            env_vars: vec![EnvVar {
                name: "DATABASE_URL".to_string(),
                description: "PostgreSQL".to_string(),
                required: true,
                default: None,
            }],
            post_install: vec!["npm install".to_string()],
        }
    }

    pub fn api_platform() -> Self {
        Self {
            name: "API Platform".to_string(),
            description:
                "Developer API with rate limiting, API keys, documentation, and webhooks"
                    .to_string(),
            template_type: TemplateType::ApiPlatform,
            tech_stack: TechStack {
                frontend: "Next.js 15 for docs + dashboard".to_string(),
                backend: "Next.js API Routes + Prisma".to_string(),
                database: "PostgreSQL + Redis".to_string(),
                auth: "API Key + OAuth for dashboard".to_string(),
                deployment: "Vercel + Upstash Redis".to_string(),
                additional: vec![
                    "Rate limiting".to_string(),
                    "Webhook delivery".to_string(),
                    "OpenAPI docs".to_string(),
                ],
            },
            features: vec![
                Feature {
                    name: "API Keys".to_string(),
                    description: "Generate, rotate, scope API keys".to_string(),
                    required: true,
                    agent_role: "auth".to_string(),
                },
                Feature {
                    name: "Rate Limiting".to_string(),
                    description: "Per-key rate limits".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Webhooks".to_string(),
                    description: "Event delivery with retry".to_string(),
                    required: true,
                    agent_role: "backend".to_string(),
                },
                Feature {
                    name: "Documentation".to_string(),
                    description: "Auto-generated API docs".to_string(),
                    required: true,
                    agent_role: "frontend".to_string(),
                },
            ],
            files: Vec::new(),
            env_vars: vec![
                EnvVar {
                    name: "DATABASE_URL".to_string(),
                    description: "PostgreSQL".to_string(),
                    required: true,
                    default: None,
                },
                EnvVar {
                    name: "REDIS_URL".to_string(),
                    description: "Redis for rate limiting".to_string(),
                    required: true,
                    default: None,
                },
            ],
            post_install: vec!["npm install".to_string()],
        }
    }

    pub fn all_templates() -> Vec<Self> {
        vec![
            Self::saas(),
            Self::ecommerce(),
            Self::marketplace(),
            Self::internal_tool(),
            Self::api_platform(),
        ]
    }
}

fn saas_files() -> Vec<TemplateFile> {
    vec![
        TemplateFile {
            path: "package.json".to_string(),
            content: serde_json::json!({
                "name": "nexus-saas-app",
                "version": "0.1.0",
                "private": true,
                "scripts": {
                    "dev": "next dev",
                    "build": "next build",
                    "start": "next start",
                    "lint": "next lint"
                }
            })
            .to_string(),
            generated: false,
        },
        TemplateFile {
            path: ".env.example".to_string(),
            content: "DATABASE_URL=postgresql://...\nNEXTAUTH_SECRET=your-secret\nSTRIPE_SECRET_KEY=sk_test_...\nSTRIPE_WEBHOOK_SECRET=whsec_...".to_string(),
            generated: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saas_template_has_correct_type() {
        let t = AppTemplate::saas();
        assert_eq!(t.template_type, TemplateType::Saas);
        assert_eq!(t.name, "SaaS Starter");
    }

    #[test]
    fn ecommerce_template_has_correct_type() {
        let t = AppTemplate::ecommerce();
        assert_eq!(t.template_type, TemplateType::Ecommerce);
    }

    #[test]
    fn marketplace_template_has_correct_type() {
        let t = AppTemplate::marketplace();
        assert_eq!(t.template_type, TemplateType::Marketplace);
    }

    #[test]
    fn internal_tool_template_has_correct_type() {
        let t = AppTemplate::internal_tool();
        assert_eq!(t.template_type, TemplateType::InternalTool);
    }

    #[test]
    fn api_platform_template_has_correct_type() {
        let t = AppTemplate::api_platform();
        assert_eq!(t.template_type, TemplateType::ApiPlatform);
    }

    #[test]
    fn all_templates_returns_five() {
        let templates = AppTemplate::all_templates();
        assert_eq!(templates.len(), 5);
    }

    #[test]
    fn saas_template_has_env_vars() {
        let t = AppTemplate::saas();
        assert!(t.env_vars.iter().any(|e| e.name == "DATABASE_URL"));
        assert!(t.env_vars.iter().any(|e| e.name == "STRIPE_SECRET_KEY"));
    }

    #[test]
    fn saas_template_has_files() {
        let t = AppTemplate::saas();
        assert!(!t.files.is_empty());
        assert!(t.files.iter().any(|f| f.path == "package.json"));
    }

    #[test]
    fn all_templates_have_features() {
        for template in AppTemplate::all_templates() {
            assert!(
                !template.features.is_empty(),
                "Template '{}' has no features",
                template.name,
            );
        }
    }

    #[test]
    fn template_serialization_roundtrip() {
        let t = AppTemplate::saas();
        let json = serde_json::to_string(&t).unwrap();
        let deserialized: AppTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, t.name);
        assert_eq!(deserialized.template_type, t.template_type);
    }

    #[test]
    fn template_type_serializes_to_snake_case() {
        let tt = TemplateType::InternalTool;
        let json = serde_json::to_string(&tt).unwrap();
        assert_eq!(json, "\"internal_tool\"");
    }
}
