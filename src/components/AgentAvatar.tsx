interface Props {
  className?: string;
  title?: string;
}

/**
 * Voice-agent avatar: a friendly robot with headphone cups and a
 * waveform "mouth", rendered in the app's amber-on-navy palette.
 */
export function AgentAvatar({ className, title = "Copilot agent" }: Props) {
  return (
    <svg
      className={className}
      viewBox="8 1 84 84"
      role="img"
      aria-label={title}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <linearGradient
          id="agentAmber"
          x1="24"
          y1="14"
          x2="76"
          y2="72"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0" stopColor="#ffe0a3" />
          <stop offset="0.5" stopColor="#ffcf72" />
          <stop offset="1" stopColor="#f5a623" />
        </linearGradient>
      </defs>

      <line x1="50" y1="30" x2="50" y2="20" stroke="url(#agentAmber)" strokeWidth="4" strokeLinecap="round" />
      <circle cx="50" cy="16" r="5" fill="url(#agentAmber)" />

      <rect x="14" y="41" width="9" height="20" rx="4.5" fill="url(#agentAmber)" />
      <rect x="77" y="41" width="9" height="20" rx="4.5" fill="url(#agentAmber)" />

      <rect x="24" y="30" width="52" height="44" rx="15" fill="url(#agentAmber)" />

      <circle cx="40" cy="47" r="5.5" fill="#131a2e" />
      <circle cx="60" cy="47" r="5.5" fill="#131a2e" />

      <g fill="#131a2e">
        <rect x="36" y="59" width="4" height="6" rx="2" />
        <rect x="42" y="55" width="4" height="14" rx="2" />
        <rect x="48" y="53" width="4" height="18" rx="2" />
        <rect x="54" y="55" width="4" height="14" rx="2" />
        <rect x="60" y="59" width="4" height="6" rx="2" />
      </g>
    </svg>
  );
}
