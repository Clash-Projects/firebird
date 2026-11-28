<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    label?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    label,
    onchange,
  }: Props = $props();
</script>

<span class="lw-switch" data-disabled={disabled}>
  <input
    type="checkbox"
    role="switch"
    aria-label={label}
    {disabled}
    bind:checked
    onchange={(event) => onchange?.(event.currentTarget.checked)}
  />
  <span class="lw-switch-track" aria-hidden="true">
    <span class="lw-switch-knob"></span>
  </span>
</span>

<style>
  .lw-switch {
    position: relative;
    display: inline-grid;
    flex: none;
    inline-size: 40px;
    block-size: 20px;
    vertical-align: middle;
  }

  .lw-switch[data-disabled="true"] {
    opacity: 0.42;
  }

  input {
    position: absolute;
    z-index: 1;
    inset: 0;
    inline-size: 100%;
    block-size: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  input:disabled {
    cursor: default;
  }

  .lw-switch-track {
    display: flex;
    align-items: center;
    padding-inline: 3px;
    border: 1px solid var(--lw-text-tertiary);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.02);
    transition:
      background-color 120ms var(--ease-native),
      border-color 120ms var(--ease-native);
  }

  :global(:root[data-theme="light"]) .lw-switch-track {
    background: rgba(0, 0, 0, 0.02);
  }

  .lw-switch-knob {
    inline-size: 12px;
    block-size: 12px;
    border-radius: 999px;
    background: var(--lw-text-secondary);
    transition:
      transform 150ms var(--ease-wave),
      inline-size 110ms var(--ease-native),
      block-size 110ms var(--ease-native),
      background-color 120ms var(--ease-native);
  }

  input:not(:disabled):hover + .lw-switch-track {
    border-color: var(--lw-text-secondary);
    background: var(--lw-wash-hover);
  }

  input:not(:disabled):hover + .lw-switch-track .lw-switch-knob {
    transform: scale(1.1);
  }

  input:not(:disabled):active + .lw-switch-track .lw-switch-knob {
    inline-size: 15px;
  }

  input:checked + .lw-switch-track {
    border-color: var(--lw-accent);
    background: var(--lw-accent);
  }

  input:checked:not(:disabled):hover + .lw-switch-track {
    border-color: var(--lw-accent);
    background: color-mix(in srgb, var(--lw-accent) 90%, transparent);
  }

  input:checked + .lw-switch-track .lw-switch-knob {
    transform: translateX(20px);
    background: var(--lw-on-accent);
  }

  input:checked:not(:disabled):hover + .lw-switch-track .lw-switch-knob {
    transform: translateX(20px) scale(1.1);
  }

  input:checked:not(:disabled):active + .lw-switch-track .lw-switch-knob {
    transform: translateX(17px);
    inline-size: 15px;
  }

  input:focus-visible + .lw-switch-track {
    outline: 2px solid var(--lw-focus-ring);
    outline-offset: 2px;
  }
</style>
