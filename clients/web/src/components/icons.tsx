/**
 * Single cross-platform linear icon set (design direction §3, §5): thin
 * stroke, `currentColor`, no emoji. macOS shells may swap SF Symbols later.
 */
import type { ReactNode } from "react";

type IconProps = {
  size?: number;
  className?: string;
};

function Icon({
  size = 18,
  className,
  children,
}: IconProps & { children: ReactNode }) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function SchoolIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 9.5 10 5l7 4.5" />
      <path d="M4.5 9v6.5h11V9" />
      <path d="M8 15.5v-4h4v4" />
    </Icon>
  );
}

export function PeopleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="8" cy="7" r="2.4" />
      <path d="M3.5 15c.4-2.6 2.2-4 4.5-4s4.1 1.4 4.5 4" />
      <circle cx="14" cy="8" r="2" />
      <path d="M13.5 11.2c1.7.3 2.8 1.6 3.1 3.3" />
    </Icon>
  );
}

export function LayoutIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3" y="3" width="14" height="11" rx="1.5" />
      <path d="M3 8h14" />
      <path d="M3 11.5h6" />
      <path d="M12 11.5h5" />
      <path d="M7.5 14v2.5M12.5 14v2.5" />
    </Icon>
  );
}

export function RulesIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 3h9v14H6z" />
      <path d="M8.5 6.5h4M8.5 9.5h4M8.5 12.5h2.5" />
    </Icon>
  );
}

export function HistoryIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="10" cy="10" r="6.5" />
      <path d="M10 6.5V10l2.5 1.5" />
      <path d="M3.5 3.5 6 6M16.5 3.5 14 6" />
    </Icon>
  );
}

export function WorkspaceIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 3.5h8l1.5 2v11H4.5v-11L6 3.5Z" />
      <path d="M8 9.5h4M10 7.5v4" />
    </Icon>
  );
}

export function CheckIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m4 10.5 4 4 8-9" />
    </Icon>
  );
}
