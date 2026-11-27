<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'chip';
    size?: 'sm' | 'md' | 'icon';
    active?: boolean;
    disabled?: boolean;
    title?: string;
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
    title,
    type = 'button',
    class: className = '',
    onclick,
    children,
  }: Props = $props();

  const variantClasses: Record<string, string> = {
    primary: 'bg-[var(--lw-accent)] text-[var(--lw-on-accent)] hover:brightness-95 font-semibold',
    secondary:
      'bg-[var(--lw-surface-raised)] text-[var(--lw-text-primary)] border border-[var(--lw-outline)] hover:bg-[var(--lw-surface-overlay)]',
    ghost: 'bg-transparent text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]',
    danger: 'bg-[var(--lw-danger)]/15 text-[var(--lw-danger)] border border-[var(--lw-danger)]/35 hover:bg-[var(--lw-danger)]/25',
    chip: 'rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]',
  };

  const sizeClasses: Record<string, string> = {
    sm: 'h-8 px-3 text-[12px]',
    md: 'h-9 px-4 text-[13px]',
    icon: 'h-9 w-9 p-0 hover:scale-[1.05] active:scale-[0.93]',
  };
</script>

<button
  {type}
  {disabled}
  {title}
  {onclick}
  class="inline-flex cursor-pointer items-center justify-center gap-2 transition-all duration-[160ms] ease-[var(--ease-native)] disabled:cursor-not-allowed disabled:opacity-45 {variant === 'chip' ? 'rounded-full' : 'rounded-[10px]'} {sizeClasses[size]} {variantClasses[variant]} {active ? '!bg-[var(--lw-accent)] !text-[var(--lw-on-accent)] !border-transparent' : ''} {className}"
>
  {@render children()}
</button>
