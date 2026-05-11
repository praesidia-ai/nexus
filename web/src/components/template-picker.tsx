'use client';

import { useState, useEffect } from 'react';
import {
  ShoppingCart,
  FileText,
  BarChart3,
  Calendar,
  MessageCircle,
  Globe,
  Briefcase,
  Heart,
  Pencil,
  CheckSquare,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { api } from '@/lib/api';
import { Badge } from '@/components/ui/badge';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Template {
  id: string;
  name: string;
  description: string;
  category: string;
  icon: string;
  preview_prompt: string;
  features: string[];
  estimated_credits: number;
}

type Category = 'All' | 'Business' | 'Personal' | 'Content' | 'Productivity';

const CATEGORIES: Category[] = ['All', 'Business', 'Personal', 'Content', 'Productivity'];

// ---------------------------------------------------------------------------
// Icon mapping
// ---------------------------------------------------------------------------

const ICON_MAP: Record<string, LucideIcon> = {
  'shopping-cart': ShoppingCart,
  'file-text': FileText,
  'bar-chart': BarChart3,
  calendar: Calendar,
  'message-circle': MessageCircle,
  globe: Globe,
  briefcase: Briefcase,
  heart: Heart,
  pencil: Pencil,
  'check-square': CheckSquare,
};

function getIcon(iconName: string): LucideIcon {
  return ICON_MAP[iconName] ?? Globe;
}

// ---------------------------------------------------------------------------
// Fallback templates (shown when backend is unavailable)
// ---------------------------------------------------------------------------

const FALLBACK_TEMPLATES: Template[] = [
  {
    id: 'saas-landing',
    name: 'SaaS Landing Page',
    description: 'A modern landing page with pricing, testimonials, and signup form.',
    category: 'Business',
    icon: 'globe',
    preview_prompt: 'Build a SaaS landing page for a project management tool with a hero section, pricing plans, testimonials, and a signup form.',
    features: ['Hero section', 'Pricing table', 'Testimonials'],
    estimated_credits: 8,
  },
  {
    id: 'ecommerce-store',
    name: 'E-Commerce Store',
    description: 'A product catalog with cart, checkout, and order management.',
    category: 'Business',
    icon: 'shopping-cart',
    preview_prompt: 'Build an e-commerce store for handmade candles with product catalog, shopping cart, checkout flow, and order tracking.',
    features: ['Product catalog', 'Shopping cart', 'Checkout'],
    estimated_credits: 15,
  },
  {
    id: 'blog-platform',
    name: 'Blog Platform',
    description: 'A content publishing platform with markdown editor and comments.',
    category: 'Content',
    icon: 'pencil',
    preview_prompt: 'Build a personal blog platform with a markdown editor, categories, comment system, and RSS feed.',
    features: ['Markdown editor', 'Categories', 'Comments'],
    estimated_credits: 10,
  },
  {
    id: 'task-manager',
    name: 'Task Manager',
    description: 'Kanban-style task board with drag-and-drop and team collaboration.',
    category: 'Productivity',
    icon: 'check-square',
    preview_prompt: 'Build a task management app with kanban board, drag-and-drop, due dates, labels, and team assignment features.',
    features: ['Kanban board', 'Drag & drop', 'Team collaboration'],
    estimated_credits: 12,
  },
  {
    id: 'personal-dashboard',
    name: 'Personal Dashboard',
    description: 'A customizable dashboard with widgets for weather, calendar, and notes.',
    category: 'Personal',
    icon: 'bar-chart',
    preview_prompt: 'Build a personal dashboard with weather widget, calendar view, quick notes, bookmarks, and a habit tracker.',
    features: ['Widgets', 'Calendar', 'Notes'],
    estimated_credits: 10,
  },
  {
    id: 'analytics-dashboard',
    name: 'Analytics Dashboard',
    description: 'Data visualization dashboard with charts, filters, and reports.',
    category: 'Business',
    icon: 'bar-chart',
    preview_prompt: 'Build an analytics dashboard with line charts, bar charts, date range filters, and downloadable PDF reports.',
    features: ['Charts', 'Filters', 'Reports'],
    estimated_credits: 12,
  },
  {
    id: 'scheduling-app',
    name: 'Scheduling App',
    description: 'Appointment booking system with calendar view and email notifications.',
    category: 'Productivity',
    icon: 'calendar',
    preview_prompt: 'Build a scheduling app like Calendly with calendar view, booking slots, email confirmations, and timezone support.',
    features: ['Calendar view', 'Booking slots', 'Notifications'],
    estimated_credits: 14,
  },
  {
    id: 'chat-support',
    name: 'Customer Support Chat',
    description: 'Live chat widget with AI auto-responses and ticket management.',
    category: 'Business',
    icon: 'message-circle',
    preview_prompt: 'Build a customer support chat system with a live chat widget, AI-powered auto-responses, agent dashboard, and ticket tracking.',
    features: ['Live chat', 'AI responses', 'Ticket management'],
    estimated_credits: 16,
  },
];

// ---------------------------------------------------------------------------
// Category color mapping
// ---------------------------------------------------------------------------

const CATEGORY_BADGE_VARIANT: Record<string, 'default' | 'success' | 'purple' | 'warning' | 'info'> = {
  Business: 'default',
  Personal: 'purple',
  Content: 'warning',
  Productivity: 'info',
};

// ---------------------------------------------------------------------------
// TemplatePicker
// ---------------------------------------------------------------------------

interface TemplatePickerProps {
  onSelect: (template: Template) => void;
  className?: string;
}

export function TemplatePicker({ onSelect, className }: TemplatePickerProps) {
  const [templates, setTemplates] = useState<Template[]>(FALLBACK_TEMPLATES);
  const [activeCategory, setActiveCategory] = useState<Category>('All');

  useEffect(() => {
    api.listTemplates()
      .then((data) => {
        if (data.templates && data.templates.length > 0) {
          setTemplates(data.templates);
        }
      })
      .catch(() => {
        // Keep fallbacks
      });
  }, []);

  const filtered =
    activeCategory === 'All'
      ? templates
      : templates.filter((t) => t.category === activeCategory);

  return (
    <div className={cn('w-full', className)}>
      {/* Category filter tabs */}
      <div className="flex items-center gap-1 mb-4 overflow-x-auto scrollbar-thin pb-1">
        {CATEGORIES.map((cat) => (
          <button
            key={cat}
            type="button"
            onClick={() => setActiveCategory(cat)}
            className={cn(
              'px-3 py-1.5 rounded-full text-xs font-medium transition-all duration-200 whitespace-nowrap',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40',
              activeCategory === cat
                ? 'bg-glow-cyan/[0.12] text-glow-cyan border border-glow-cyan/20'
                : 'text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent',
            )}
          >
            {cat}
          </button>
        ))}
      </div>

      {/* Template grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
        {filtered.map((template) => {
          const Icon = getIcon(template.icon);
          const badgeVariant = CATEGORY_BADGE_VARIANT[template.category] ?? 'secondary';

          return (
            <button
              key={template.id}
              type="button"
              onClick={() => onSelect(template)}
              className={cn(
                'group relative flex flex-col items-start text-left p-4 rounded-xl',
                'border border-white/[0.06] bg-white/[0.02]',
                'hover:border-glow-cyan/20 hover:bg-white/[0.04]',
                'hover:shadow-[0_0_20px_rgba(0,255,255,0.05)]',
                'transition-all duration-200',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-glow-cyan/40',
              )}
            >
              {/* Icon + badges row */}
              <div className="flex items-start justify-between w-full mb-3">
                <div className="w-9 h-9 rounded-lg bg-glow-cyan/[0.08] border border-glow-cyan/10 flex items-center justify-center flex-shrink-0">
                  <Icon className="w-4.5 h-4.5 text-glow-cyan" />
                </div>
                <div className="flex items-center gap-1.5">
                  <Badge variant={badgeVariant} className="text-[9px] px-1.5 py-0">
                    {template.category}
                  </Badge>
                </div>
              </div>

              {/* Name */}
              <h3 className="text-sm font-medium text-slate-200 mb-1 group-hover:text-glow-cyan transition-colors">
                {template.name}
              </h3>

              {/* Description (2 lines max) */}
              <p className="text-[11px] text-slate-400 leading-relaxed line-clamp-2 mb-3 flex-1">
                {template.description}
              </p>

              {/* Footer: credits badge + CTA */}
              <div className="flex items-center justify-between w-full mt-auto">
                <span className="text-[10px] text-slate-500">
                  ~{template.estimated_credits} credits
                </span>
                <span className="text-[10px] text-glow-cyan opacity-0 group-hover:opacity-100 transition-opacity">
                  Use template &rarr;
                </span>
              </div>
            </button>
          );
        })}
      </div>

      {filtered.length === 0 && (
        <p className="text-xs text-slate-500 text-center py-8">
          No templates in this category yet.
        </p>
      )}
    </div>
  );
}
