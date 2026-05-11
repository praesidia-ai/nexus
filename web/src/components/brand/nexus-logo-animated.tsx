"use client";

import { useRef, useEffect } from "react";
import * as THREE from "three";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Geometry helpers — convert the SVG petal bezier math to 3D tube meshes
// ---------------------------------------------------------------------------

const SVG_C = 200;
const WORLD_SCALE = 0.027;

function toV3(sx: number, sy: number): THREE.Vector3 {
  return new THREE.Vector3(
    (sx - SVG_C) * WORLD_SCALE,
    -(sy - SVG_C) * WORLD_SCALE,
    0,
  );
}

function cubicBezierPoint(
  p0: THREE.Vector3,
  p1: THREE.Vector3,
  p2: THREE.Vector3,
  p3: THREE.Vector3,
  t: number,
): THREE.Vector3 {
  const mt = 1 - t;
  return new THREE.Vector3(
    mt * mt * mt * p0.x +
      3 * mt * mt * t * p1.x +
      3 * mt * t * t * p2.x +
      t * t * t * p3.x,
    mt * mt * mt * p0.y +
      3 * mt * mt * t * p1.y +
      3 * mt * t * t * p2.y +
      t * t * t * p3.y,
    mt * mt * mt * p0.z +
      3 * mt * mt * t * p1.z +
      3 * mt * t * t * p2.z +
      t * t * t * p3.z,
  );
}

function petalPoints(
  angleDeg: number,
  r: number,
  spread: number,
  twist = 8,
  samples = 48,
): THREE.Vector3[] {
  const c = SVG_C;
  const rad = (angleDeg * Math.PI) / 180;
  const radR = ((angleDeg + 180 + twist) * Math.PI) / 180;
  const sRad = (spread * Math.PI) / 180;

  const ex = Math.cos(rad);
  const ey = Math.sin(rad);
  const rx = Math.cos(radR);
  const ry = Math.sin(radR);

  const tipRad = ((angleDeg + 90) * Math.PI) / 180;
  const tipX = c + Math.cos(tipRad) * r * 0.95;
  const tipY = c + Math.sin(tipRad) * r * 0.95;

  const cp1x = c + ex * r * 0.7 + Math.cos(rad + sRad) * spread * 0.9;
  const cp1y = c + ey * r * 0.7 + Math.sin(rad + sRad) * spread * 0.9;
  const cp2x = tipX + Math.cos(rad) * r * 0.3;
  const cp2y = tipY + Math.sin(rad) * r * 0.3;

  const cp3x = tipX + Math.cos(radR) * r * 0.3;
  const cp3y = tipY + Math.sin(radR) * r * 0.3;
  const cp4x = c + rx * r * 0.7 + Math.cos(radR - sRad) * spread * 0.9;
  const cp4y = c + ry * r * 0.7 + Math.sin(radR - sRad) * spread * 0.9;

  const start = toV3(c, c);
  const p1 = toV3(cp1x, cp1y);
  const p2 = toV3(cp2x, cp2y);
  const tip = toV3(tipX, tipY);
  const p3 = toV3(cp3x, cp3y);
  const p4 = toV3(cp4x, cp4y);
  const end = toV3(c, c);

  const half = Math.floor(samples / 2);
  const pts: THREE.Vector3[] = [];

  for (let i = 0; i <= half; i++) {
    pts.push(cubicBezierPoint(start, p1, p2, tip, i / half));
  }
  for (let i = 1; i <= half; i++) {
    pts.push(cubicBezierPoint(tip, p3, p4, end, i / half));
  }

  return pts;
}

function makeTube(
  pts: THREE.Vector3[],
  tubeRadius: number,
  color: string,
  opacity: number,
): THREE.Mesh {
  const curve = new THREE.CatmullRomCurve3(pts, false, "catmullrom", 0.3);
  const geo = new THREE.TubeGeometry(curve, 80, tubeRadius, 8, false);
  const mat = new THREE.MeshBasicMaterial({
    color: new THREE.Color(color),
    transparent: true,
    opacity,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });
  return new THREE.Mesh(geo, mat);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface NexusLogoAnimatedProps {
  size?: number;
  className?: string;
}

export function NexusLogoAnimated({
  size = 200,
  className,
}: NexusLogoAnimatedProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    cleanupRef.current?.();

    const w = size;
    const h = size;
    const dpr = Math.min(window.devicePixelRatio, 2);

    // --- Scene / camera / renderer ---------------------------------------
    const scene = new THREE.Scene();
    const frustum = 5.5;
    const camera = new THREE.OrthographicCamera(
      -frustum,
      frustum,
      frustum,
      -frustum,
      0.1,
      100,
    );
    camera.position.z = 10;

    const renderer = new THREE.WebGLRenderer({
      antialias: true,
      powerPreference: "high-performance",
    });
    renderer.setSize(w, h);
    renderer.setPixelRatio(dpr);
    renderer.setClearColor(0x0a0e1a, 1);
    renderer.toneMapping = THREE.NoToneMapping;
    container.appendChild(renderer.domElement);
    renderer.domElement.style.pointerEvents = "none";

    // --- Layer groups ----------------------------------------------------
    const outerGroup = new THREE.Group();
    const midGroup = new THREE.Group();
    const blueGroup = new THREE.Group();
    const coreGroup = new THREE.Group();
    const centerGroup = new THREE.Group();
    scene.add(outerGroup, midGroup, blueGroup, coreGroup, centerGroup);

    const allGroups = [
      centerGroup,
      coreGroup,
      blueGroup,
      midGroup,
      outerGroup,
    ];
    allGroups.forEach((g) => g.scale.setScalar(0));

    // --- Petal tube layers -----------------------------------------------
    // Same angles / params as the SVG logo for a faithful match.

    const angles6 = [0, 60, 120, 180, 240, 300];
    const angles5 = [0, 72, 144, 216, 288];
    const angles4 = [0, 90, 180, 270];

    // OUTER: gold/amber, r=145, spread=30, twist=10
    angles6.forEach((a, i) => {
      const pts = petalPoints(a + 5, 145, 30, 10);
      outerGroup.add(makeTube(pts, 0.055, "#d4a020", 0.06));
      outerGroup.add(
        makeTube(
          pts,
          0.022,
          i % 2 === 0 ? "#e8b830" : "#cc9420",
          0.4,
        ),
      );
    });

    // MID: purple/violet, r=135, spread=28, twist=8
    angles6.forEach((a, i) => {
      const pts = petalPoints(a - 3, 135, 28, 8);
      midGroup.add(makeTube(pts, 0.06, "#9333ea", 0.06));
      midGroup.add(
        makeTube(
          pts,
          0.02,
          i % 2 === 0 ? "#a855f7" : "#7c3aed",
          0.55,
        ),
      );
    });

    // BLUE: r=115, spread=24, twist=6
    angles5.forEach((a, i) => {
      const pts = petalPoints(a, 115, 24, 6);
      blueGroup.add(makeTube(pts, 0.055, "#2563eb", 0.06));
      blueGroup.add(
        makeTube(
          pts,
          0.025,
          i % 2 === 0 ? "#60a5fa" : "#3b82f6",
          0.6,
        ),
      );
    });

    // CORE: cyan, r=85, spread=18, twist=5
    angles4.forEach((a) => {
      const pts = petalPoints(a + 2, 85, 18, 5);
      coreGroup.add(makeTube(pts, 0.07, "#00c8f0", 0.06));
      coreGroup.add(makeTube(pts, 0.03, "#22d3ee", 0.75));
    });

    // --- Center starburst ------------------------------------------------
    const glowMat = new THREE.MeshBasicMaterial({
      color: new THREE.Color("#00d4ff"),
      transparent: true,
      opacity: 0.25,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Mesh(
      new THREE.SphereGeometry(0.5, 32, 32),
      glowMat,
    );
    centerGroup.add(glow);

    const coreSphere = new THREE.Mesh(
      new THREE.SphereGeometry(0.12, 32, 32),
      new THREE.MeshBasicMaterial({ color: 0xffffff }),
    );
    centerGroup.add(coreSphere);

    // Cardinal rays
    [0, 90, 180, 270].forEach((deg) => {
      const rad = (deg * Math.PI) / 180;
      const len = 0.9;
      const rpts = [
        new THREE.Vector3(0, 0, 0),
        new THREE.Vector3(Math.cos(rad) * len, Math.sin(rad) * len, 0),
      ];
      const rc = new THREE.LineCurve3(rpts[0], rpts[1]);
      const rg = new THREE.TubeGeometry(rc, 2, 0.018, 6, false);
      const rm = new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.45,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      });
      centerGroup.add(new THREE.Mesh(rg, rm));
    });

    // Diagonal rays (shorter)
    [45, 135, 225, 315].forEach((deg) => {
      const rad = (deg * Math.PI) / 180;
      const len = 0.5;
      const rpts = [
        new THREE.Vector3(0, 0, 0),
        new THREE.Vector3(Math.cos(rad) * len, Math.sin(rad) * len, 0),
      ];
      const rc = new THREE.LineCurve3(rpts[0], rpts[1]);
      const rg = new THREE.TubeGeometry(rc, 2, 0.012, 6, false);
      const rm = new THREE.MeshBasicMaterial({
        color: new THREE.Color("#bae6fd"),
        transparent: true,
        opacity: 0.3,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      });
      centerGroup.add(new THREE.Mesh(rg, rm));
    });

    // --- Post-processing: bloom ------------------------------------------
    const composer = new EffectComposer(renderer);
    composer.addPass(new RenderPass(scene, camera));
    const bloomPass = new UnrealBloomPass(
      new THREE.Vector2(w * dpr, h * dpr),
      0.9,
      0.4,
      0.25,
    );
    composer.addPass(bloomPass);
    composer.addPass(new OutputPass());

    // --- Animation -------------------------------------------------------
    const clock = new THREE.Clock();
    let frameId = 0;

    const entranceDelays = [0, 0.08, 0.16, 0.26, 0.38];
    const entranceDurations = [0.4, 0.5, 0.6, 0.7, 0.8];

    function easeOutCubic(x: number): number {
      return 1 - Math.pow(1 - x, 3);
    }

    function animate() {
      frameId = requestAnimationFrame(animate);
      const t = clock.getElapsedTime();

      // Staggered entrance
      for (let i = 0; i < allGroups.length; i++) {
        const p = (t - entranceDelays[i]) / entranceDurations[i];
        allGroups[i].scale.setScalar(
          p >= 1 ? 1 : easeOutCubic(Math.max(0, Math.min(1, p))),
        );
      }

      // Slow layer rotations matching the SVG speeds
      outerGroup.rotation.z = t * 0.018;
      midGroup.rotation.z = -t * 0.022;
      blueGroup.rotation.z = t * 0.016;
      coreGroup.rotation.z = -t * 0.035;

      // Breathing bloom
      bloomPass.strength = 0.85 + 0.15 * Math.sin(t * 0.6);

      // Center pulse
      const pulse = 0.85 + 0.15 * Math.sin(t * 1.0);
      glowMat.opacity = 0.25 * pulse;
      glow.scale.setScalar(1.0 + 0.06 * Math.sin(t * 0.8));

      composer.render();
    }

    animate();

    // --- Cleanup ---------------------------------------------------------
    const dispose = () => {
      cancelAnimationFrame(frameId);
      composer.dispose();
      scene.traverse((obj) => {
        if (obj instanceof THREE.Mesh) {
          obj.geometry.dispose();
          const m = obj.material;
          if (Array.isArray(m)) {
            m.forEach((x) => x.dispose());
          } else {
            m.dispose();
          }
        }
      });
      renderer.dispose();
      renderer.domElement.remove();
    };

    cleanupRef.current = dispose;
    return dispose;
  }, [size]);

  return (
    <div
      ref={containerRef}
      className={cn(
        "relative inline-flex items-center justify-center",
        className,
      )}
      style={{ width: size, height: size }}
    />
  );
}
