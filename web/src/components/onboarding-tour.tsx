'use client';

import { useState, useEffect, useCallback } from 'react';
import { Sparkles, MessageSquare, Eye, Users, Rocket, X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { api } from '@/lib/api';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface TourStep {
  title: string;
  description: string;
  icon: typeof Sparkles;
  /** CSS selector for the spotlight target element */
  spotlightSelector?: string;
}

const STEPS: TourStep[] = [
  {
    title: 'Welcome to Nexus!',
    description: 'Build apps and run your business with AI. No coding required.',
    icon: Sparkles,
  },
  {
    title: 'Chat with Nexus',
    description: 'Describe what you want. Nexus builds it.',
    icon: MessageSquare,
    spotlightSelector: '[data-tour="chat-area"]',
  },
  {
    title: 'Preview your app',
    description: 'See your app come to life in real-time.',
    icon: Eye,
    spotlightSelector: '[data-tour="preview-tab"]',
  },
  {
    title: 'AI Business Teams',
    description: 'Set up teams to run support, marketing, and more.',
    icon: Users,
    spotlightSelector: '[data-tour="business-nav"]',
  },
  {
    title: "You're all set!",
    description: 'Start by describing your first app below.',
    icon: Rocket,
  },
];

const STORAGE_KEY = 'nexus-onboarding-complete';

// ---------------------------------------------------------------------------
// OnboardingTour
// ---------------------------------------------------------------------------

interface OnboardingTourProps {
  onComplete: () => void;
  onSkip: () => void;
}

export function OnboardingTour({ onComplete, onSkip }: OnboardingTourProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [spotlightRect, setSpotlightRect] = useState<DOMRect | null>(null);

  const step = STEPS[currentStep];
  const Icon = step.icon;
  const isLast = currentStep === STEPS.length - 1;

  // Update spotlight position when step changes
  useEffect(() => {
    if (!step.spotlightSelector) {
      setSpotlightRect(null);
      return;
    }

    const el = document.querySelector(step.spotlightSelector);
    if (el) {
      setSpotlightRect(el.getBoundingClientRect());
    } else {
      setSpotlightRect(null);
    }
  }, [currentStep, step.spotlightSelector]);

  const handleNext = useCallback(() => {
    if (isLast) {
      finishTour();
      onComplete();
    } else {
      const nextStep = currentStep + 1;
      setCurrentStep(nextStep);
      // Report step progress to backend (fire and forget)
      api.completeOnboardingStep(nextStep).catch(() => {});
    }
  }, [currentStep, isLast, onComplete]);

  const handleSkip = useCallback(() => {
    finishTour();
    onSkip();
  }, [onSkip]);

  function finishTour() {
    localStorage.setItem(STORAGE_KEY, 'true');
    api.completeOnboardingStep(STEPS.length).catch(() => {});
  }

  // Close on Escape
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') handleSkip();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [handleSkip]);

  return (
    <div className="fixed inset-0 z-[100]" role="dialog" aria-label="Onboarding tour">
      {/* Semi-transparent overlay */}
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" />

      {/* Spotlight cutout */}
      {spotlightRect && (
        <div
          className="absolute rounded-xl border-2 border-glow-cyan/40 shadow-glow-md z-[101] pointer-events-none transition-all duration-300"
          style={{
            top: spotlightRect.top - 8,
            left: spotlightRect.left - 8,
            width: spotlightRect.width + 16,
            height: spotlightRect.height + 16,
          }}
        />
      )}

      {/* Tour card */}
      <div className="absolute inset-0 flex items-center justify-center z-[102]">
        <div className="w-full max-w-md mx-4 rounded-2xl border border-white/[0.1] bg-[#0d0d12]/95 backdrop-blur-xl shadow-2xl overflow-hidden">
          {/* Close / skip */}
          <div className="flex items-center justify-end px-4 pt-3">
            {!isLast && (
              <button
                onClick={handleSkip}
                className="flex items-center gap-1 text-[11px] text-slate-500 hover:text-slate-300 transition-colors"
              >
                Skip tour
                <X className="w-3 h-3" />
              </button>
            )}
          </div>

          {/* Content */}
          <div className="flex flex-col items-center text-center px-8 pb-6 pt-2">
            {/* Icon */}
            <div className="w-14 h-14 rounded-2xl bg-glow-cyan/[0.1] border border-glow-cyan/20 flex items-center justify-center mb-4">
              <Icon className="w-7 h-7 text-glow-cyan" />
            </div>

            {/* Title */}
            <h2 className="text-lg font-semibold text-slate-100 mb-2">
              {step.title}
            </h2>

            {/* Description */}
            <p className="text-sm text-slate-400 leading-relaxed max-w-xs">
              {step.description}
            </p>
          </div>

          {/* Footer */}
          <div className="flex items-center justify-between px-6 py-4 border-t border-white/[0.06]">
            {/* Step indicator */}
            <div className="flex items-center gap-1.5">
              {STEPS.map((_, i) => (
                <div
                  key={i}
                  className={cn(
                    'w-1.5 h-1.5 rounded-full transition-all duration-200',
                    i === currentStep
                      ? 'w-4 bg-glow-cyan'
                      : i < currentStep
                        ? 'bg-glow-cyan/40'
                        : 'bg-white/[0.1]',
                  )}
                />
              ))}
              <span className="ml-2 text-[10px] text-slate-500">
                {currentStep + 1}/{STEPS.length}
              </span>
            </div>

            {/* Next / Finish button */}
            <button
              onClick={handleNext}
              className={cn(
                'px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-glow-cyan/40',
                isLast
                  ? 'bg-gradient-to-r from-glow-cyan to-glow-blue text-white shadow-glow-sm hover:brightness-110'
                  : 'bg-white/[0.06] text-slate-200 hover:bg-white/[0.1]',
              )}
            >
              {isLast ? "Let's go!" : 'Next'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helper: check if onboarding should show
// ---------------------------------------------------------------------------

export function useOnboardingStatus() {
  const [shouldShow, setShouldShow] = useState(false);

  useEffect(() => {
    // Check localStorage first (instant)
    const completed = localStorage.getItem(STORAGE_KEY);
    if (completed === 'true') {
      setShouldShow(false);
      return;
    }

    // Check backend preference
    api.getPreferences()
      .then((prefs) => {
        if (!prefs.onboarding_completed) {
          setShouldShow(true);
        }
      })
      .catch(() => {
        // If backend is unavailable, show for first-time users
        if (!completed) {
          setShouldShow(true);
        }
      });
  }, []);

  return { shouldShow, dismiss: () => setShouldShow(false) };
}
