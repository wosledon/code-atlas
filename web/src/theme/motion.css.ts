export const globalMotion = `
@keyframes atlasFadeUp {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes atlasPop {
  0% { transform: scale(0.96); opacity: 0; }
  100% { transform: scale(1); opacity: 1; }
}
@keyframes atlasPulse {
  0%, 100% { opacity: 0.55; }
  50% { opacity: 1; }
}
@keyframes atlasDash {
  to { stroke-dashoffset: -24; }
}
.atlas-fade { animation: atlasFadeUp 0.35s ease both; }
.atlas-pop { animation: atlasPop 0.28s ease both; }
.atlas-stagger > * { animation: atlasFadeUp 0.4s ease both; }
.atlas-stagger > *:nth-child(1) { animation-delay: 0.02s; }
.atlas-stagger > *:nth-child(2) { animation-delay: 0.06s; }
.atlas-stagger > *:nth-child(3) { animation-delay: 0.1s; }
.atlas-stagger > *:nth-child(4) { animation-delay: 0.14s; }
.atlas-stagger > *:nth-child(5) { animation-delay: 0.18s; }
.atlas-stagger > *:nth-child(6) { animation-delay: 0.22s; }
.atlas-stagger > *:nth-child(7) { animation-delay: 0.26s; }
.atlas-stagger > *:nth-child(8) { animation-delay: 0.3s; }

/* Soft scrollbars across the app */
* { scrollbar-width: thin; scrollbar-color: rgba(26, 111, 181, 0.28) transparent; }
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: linear-gradient(180deg, rgba(26,111,181,0.35), rgba(61,139,110,0.3));
  border-radius: 999px;
  border: 2px solid transparent;
  background-clip: padding-box;
}
::-webkit-scrollbar-thumb:hover {
  background: linear-gradient(180deg, rgba(26,111,181,0.55), rgba(61,139,110,0.45));
  background-clip: padding-box;
}
::-webkit-scrollbar-corner { background: transparent; }
`;
