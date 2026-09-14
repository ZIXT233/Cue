import type { AnchorHTMLAttributes, ReactNode } from "react";

interface Props extends AnchorHTMLAttributes<HTMLAnchorElement> {
  href: string;
  children?: ReactNode;
}

export default function Link({ href, children, onClick, ...props }: Props) {
  return (
    <a
      href={href}
      onClick={(event) => {
        onClick?.(event);
        if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
        if (href.startsWith("http") || href.startsWith("mailto:")) return;
        event.preventDefault();
        const next = new URL(href, window.location.href);
        window.history.pushState({}, "", `${next.pathname}${next.search}${next.hash}`);
        window.dispatchEvent(new Event("cue:location"));
      }}
      {...props}
    >
      {children}
    </a>
  );
}
