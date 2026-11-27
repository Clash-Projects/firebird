<script lang="ts">
  import type { Snippet } from 'svelte';
  import { waveTooltip } from './tooltip';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'chip';
    size?: 'sm' | 'md' | 'icon';
    active?: boolean;
    disabled?: boolean;
    tooltip?: string;
    type?: 'button' | 'submit';
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    active = false,
    disabled = false,
    tooltip,
    type = 'button',
    class: className = '',
    onclick,
    children,
  }: Props = $props();

  const variantClasses: Record<string, string> = {
    primary:
      'bg-[var(--lw-accent)] text-[var(--lw-on-accent)] font-semibold hover:opacity-90',
    secondary:
      'bg-[var(--lw-surface-raised)] text-[var(--lw-text-primary)] border border-[var(--lw-outline)] hover:bg-[var(--lw-surface-overlay)]',
    ghost:
      'bg-transparent text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]',
    danger:
      'bg-[var(--lw-danger)]/15 text-[var(--lw-danger)] border border-[var(--lw-danger)]/35 hover:bg-[var(--lw-danger)]/25',
    chip:
      'rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]',
  };

  const sizeClasses: Record<string, string> = {
    sm: 'h-8 px-3 text-[12px]',
    md: 'h-9 px-4 text-[13px]',
    icon: 'h-9 w-9 p-0',
  };
</script>

<button
  {type}
  {disabled}
  {onclick}
  use:waveTooltip={tooltip}
  aria-pressed={variant === 'chip' || active ? active : undefined}
  class="lw-press inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 text-center leading-none whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-45 {variant === 'chip' ? 'rounded-full' : 'rounded-[8px]'} {sizeClasses[size]} {variantClasses[variant]} {active ? '!bg-[var(--lw-accent)] !text-[var(--lw-on-accent)] !border-transparent' : ''} {className}"
>
  {@render children()}
</button>
