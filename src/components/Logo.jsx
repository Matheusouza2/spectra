export default function Logo({ size = 28, className = '' }) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden="true">
      <g strokeWidth="2.5" strokeLinecap="round">
        <path d="M22 32H29" stroke="var(--text-muted)" />
        <path d="M35 29L42 19" stroke="#e88d3d" />
        <path d="M35 32H42" stroke="#9a7fe0" />
        <path d="M35 35L42 45" stroke="#5a8fec" />
      </g>
      <rect x="29" y="22" width="6" height="20" rx="1.5" fill="var(--text-main)" />
      <circle cx="16" cy="32" r="6.5" stroke="var(--text-main)" strokeWidth="2.5" />
      <circle cx="16" cy="32" r="2.4" fill="#e88d3d" />
      <circle cx="48" cy="16" r="5" stroke="#e88d3d" strokeWidth="2.5" />
      <circle cx="50" cy="32" r="5" stroke="#9a7fe0" strokeWidth="2.5" />
      <circle cx="48" cy="48" r="5" stroke="#5a8fec" strokeWidth="2.5" />
    </svg>
  );
}
