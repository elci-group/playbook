//! The gallery stylesheet: NFL-playbook chrome with CSS-only animation.

/// The `<style>` block embedded in the gallery page.
pub fn styles() -> String {
    String::from(
        r#"<style>
:root {
  --turf: #1e5c34;
  --turf-dark: #17492a;
  --chalk: #f2f0e4;
  --offense: #ffd23f;
  --defense: #ff5d5d;
  --route: #7fd8ff;
  --motion: #b8ff7f;
  --rush: #ff8c42;
  --ink: #0b1512;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: radial-gradient(ellipse at top, #12241b 0%, #0b1512 70%);
  color: var(--chalk);
  font-family: 'Consolas','Menlo',monospace;
}
.stadium-header {
  text-align: center;
  padding: 28px 12px 10px;
  border-bottom: 2px solid #2c4a38;
}
.stadium-header h1 {
  margin: 0;
  font-family: 'Arial Black','Helvetica Neue',sans-serif;
  letter-spacing: 0.35em;
  font-size: 2.2rem;
  color: var(--offense);
  text-shadow: 0 0 18px rgba(255,210,63,0.25);
}
.tagline { margin: 6px 0 0; color: #9dc4ad; font-size: 0.85rem; }
.threeform-feed {
  margin: 14px auto;
  max-width: 980px;
  background: #0a120d;
  border: 1px solid #2c4a38;
  border-radius: 6px;
  padding: 10px 16px;
  font-size: 0.72rem;
  color: #86e29b;
  overflow-x: auto;
}
.threeform-feed h2 {
  font-size: 0.7rem;
  letter-spacing: 0.3em;
  color: #5f8f74;
  margin: 0 0 6px;
}
.threeform-feed pre { margin: 0; }
.card-wall {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
  gap: 22px;
  max-width: 1180px;
  margin: 18px auto;
  padding: 0 16px 30px;
}
.play-card {
  background: #0e1d15;
  border: 1px solid #2c4a38;
  border-radius: 10px;
  padding: 10px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.45);
}
.play-card svg { width: 100%; height: auto; display: block; }
.turf { fill: var(--turf); stroke: #0d2b18; stroke-width: 1; }
.endzone { fill: var(--turf-dark); }
.endzone-text {
  fill: #e8e4d0;
  font-family: 'Arial Black',sans-serif;
  font-size: 6px;
  letter-spacing: 0.6em;
  opacity: 0.85;
}
.yard-line { stroke: #d8d4be; stroke-width: 0.35; opacity: 0.55; }
.hash { stroke: #d8d4be; stroke-width: 0.3; opacity: 0.35; }
.los { stroke: #ffffff; stroke-width: 0.7; }
.los-text { fill: #ffffff; font-size: 3px; opacity: 0.8; }
.glyph-o {
  fill: var(--offense);
  stroke: #8a6d00;
  stroke-width: 0.4;
  opacity: 0;
  animation: player-in 0.5s ease-out forwards;
}
.glyph-qb {
  fill: none;
  stroke: var(--offense);
  stroke-width: 0.6;
  opacity: 0;
  animation: player-in 0.5s ease-out forwards;
}
.glyph-x {
  stroke: var(--defense);
  stroke-width: 0.7;
  stroke-linecap: round;
  opacity: 0;
  animation: player-in 0.5s ease-out forwards;
}
.glyph-label {
  fill: var(--chalk);
  font-size: 3.1px;
  font-weight: bold;
  opacity: 0;
  animation: label-in 0.5s ease-out 0.4s forwards;
}
.ball { fill: #a05a2c; stroke: #5e3418; stroke-width: 0.2; }
.card-title {
  fill: var(--offense);
  font-family: 'Arial Black',sans-serif;
  font-size: 7.5px;
  letter-spacing: 0.12em;
}
.class-chip, .side-chip {
  fill: #9dc4ad;
  font-size: 3.4px;
  letter-spacing: 0.25em;
}
.route-pass {
  fill: none;
  stroke: var(--route);
  stroke-width: 0.8;
  stroke-dasharray: 3 2;
  stroke-dashoffset: 10;
  animation: route-march 1.6s linear infinite;
}
.route-run {
  fill: none;
  stroke: var(--chalk);
  stroke-width: 1.3;
}
.route-block {
  fill: none;
  stroke: #c9b8ff;
  stroke-width: 0.8;
}
.route-motion {
  fill: none;
  stroke: var(--motion);
  stroke-width: 0.8;
  stroke-dasharray: 2 1.5;
  stroke-dashoffset: 7;
  animation: route-march 1.2s linear infinite;
}
.route-coverage {
  fill: none;
  stroke: #d8d4be;
  stroke-width: 0.6;
  stroke-dasharray: 1.5 1;
  opacity: 0.8;
}
.route-rush {
  fill: none;
  stroke: var(--rush);
  stroke-width: 1.1;
  stroke-dasharray: 2.5 1.5;
  stroke-dashoffset: 8;
  animation: rush-pulse 0.9s ease-in-out infinite;
}
.route-arrow { fill: var(--route); }
@keyframes route-march {
  to { stroke-dashoffset: 0; }
}
@keyframes rush-pulse {
  0%, 100% { stroke-dashoffset: 8; opacity: 0.7; }
  50% { stroke-dashoffset: 4; opacity: 1; }
}
@keyframes player-in {
  to { opacity: 1; }
}
@keyframes label-in {
  to { opacity: 1; }
}
.stadium-footer {
  text-align: center;
  padding: 18px;
  color: #5f8f74;
  font-size: 0.72rem;
  border-top: 1px solid #2c4a38;
}
@media (prefers-reduced-motion: reduce) {
  .route-pass, .route-motion, .route-rush { animation: none; stroke-dashoffset: 0; }
  .animated-player, .glyph-label { animation: none; opacity: 1; }
}
</style>
"#,
    )
}
