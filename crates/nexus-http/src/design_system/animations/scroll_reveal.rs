//! ScrollReveal — a React component that animates children into view on scroll.
//!
//! Emits a complete, self-contained React component using the Intersection
//! Observer API (no external animation library required).

/// Returns the full source code of a `ScrollReveal` React component.
///
/// This component wraps any children and fades/slides them in when they
/// enter the viewport. It supports configurable direction, delay, and
/// duration via props.
///
/// # Usage in generated code
///
/// ```tsx
/// import { ScrollReveal } from "@/components/scroll-reveal";
///
/// <ScrollReveal direction="up" delay={0.1}>
///   <Card>...</Card>
/// </ScrollReveal>
/// ```
pub fn scroll_reveal_component() -> &'static str {
    r#""use client";

import { useEffect, useRef, useState, type ReactNode } from "react";

type Direction = "up" | "down" | "left" | "right" | "none";

interface ScrollRevealProps {
  children: ReactNode;
  /** Animation direction. Default: "up" */
  direction?: Direction;
  /** Delay in seconds before the animation starts. Default: 0 */
  delay?: number;
  /** Animation duration in seconds. Default: 0.6 */
  duration?: number;
  /** Distance in pixels for the translate. Default: 24 */
  distance?: number;
  /** IntersectionObserver threshold. Default: 0.1 */
  threshold?: number;
  /** CSS class to add to the wrapper. */
  className?: string;
}

function getTranslate(direction: Direction, distance: number): string {
  switch (direction) {
    case "up":
      return `translateY(${distance}px)`;
    case "down":
      return `translateY(-${distance}px)`;
    case "left":
      return `translateX(${distance}px)`;
    case "right":
      return `translateX(-${distance}px)`;
    case "none":
      return "none";
  }
}

export function ScrollReveal({
  children,
  direction = "up",
  delay = 0,
  duration = 0.6,
  distance = 24,
  threshold = 0.1,
  className = "",
}: ScrollRevealProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true);
          observer.unobserve(el);
        }
      },
      { threshold }
    );

    observer.observe(el);
    return () => observer.disconnect();
  }, [threshold]);

  return (
    <div
      ref={ref}
      className={className}
      style={{
        opacity: isVisible ? 1 : 0,
        transform: isVisible ? "none" : getTranslate(direction, distance),
        transition: `opacity ${duration}s ease ${delay}s, transform ${duration}s ease ${delay}s`,
        willChange: "opacity, transform",
      }}
    >
      {children}
    </div>
  );
}
"#
}

/// Returns a staggered container component that applies incremental delays
/// to each child `ScrollReveal` wrapper.
pub fn stagger_container_component() -> &'static str {
    r#""use client";

import { Children, type ReactNode } from "react";
import { ScrollReveal } from "@/components/scroll-reveal";

interface StaggerContainerProps {
  children: ReactNode;
  /** Delay increment per child in seconds. Default: 0.1 */
  stagger?: number;
  /** Animation direction for each child. Default: "up" */
  direction?: "up" | "down" | "left" | "right" | "none";
  /** CSS class for the container. */
  className?: string;
}

export function StaggerContainer({
  children,
  stagger = 0.1,
  direction = "up",
  className = "",
}: StaggerContainerProps) {
  return (
    <div className={className}>
      {Children.map(children, (child, index) => (
        <ScrollReveal direction={direction} delay={index * stagger}>
          {child}
        </ScrollReveal>
      ))}
    </div>
  );
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_reveal_is_valid_component() {
        let code = scroll_reveal_component();
        assert!(code.contains("export function ScrollReveal"));
        assert!(code.contains("IntersectionObserver"));
        assert!(code.contains("useEffect"));
    }

    #[test]
    fn stagger_container_imports_scroll_reveal() {
        let code = stagger_container_component();
        assert!(code.contains("import { ScrollReveal }"));
        assert!(code.contains("export function StaggerContainer"));
    }
}
