<script lang="ts">
  let { checked = $bindable(false), label, onchange }: {
    checked?: boolean;
    label?: string;
    onchange?: (checked: boolean) => void;
  } = $props();
</script>

<span class="switch">
  <span class="switch-state" aria-hidden="true">{checked ? 'On' : 'Off'}</span>
  <span class="switch-control">
    <input
      type="checkbox"
      role="switch"
      aria-label={label}
      bind:checked
      onchange={(event) => onchange?.(event.currentTarget.checked)}
    />
    <span class="switch-track" aria-hidden="true"></span>
  </span>
</span>

<style>
  .switch { display: inline-flex; flex: none; align-items: center; gap: 10px; }
  .switch-state { min-inline-size: 2em; color: var(--lw-text-secondary); font-size: 12.5px; text-align: end; }
  .switch-control { position: relative; display: grid; inline-size: 40px; block-size: 22px; }
  input { position: absolute; z-index: 1; inset: 0; inline-size: 100%; block-size: 100%; margin: 0; opacity: 0; cursor: pointer; }

  .switch-track {
    display: flex;
    align-items: center;
    padding-inline: 3px;
    border: 1px solid var(--lw-text-tertiary);
    border-radius: 999px;
    transition: background-color var(--lw-motion-fast), border-color var(--lw-motion-fast);
  }

  .switch-track::before {
    inline-size: 12px;
    block-size: 12px;
    border-radius: 50%;
    background: var(--lw-text-tertiary);
    content: '';
    transition: transform var(--lw-motion-normal) var(--ease-wave), background-color var(--lw-motion-fast);
  }

  input:checked + .switch-track { border-color: var(--lw-accent); background: var(--lw-accent); }
  input:checked + .switch-track::before { transform: translateX(20px); background: var(--lw-on-accent); }
  input:dir(rtl):checked + .switch-track::before { transform: translateX(-20px); }
  input:focus-visible + .switch-track { outline: 2px solid var(--lw-focus-ring); outline-offset: 3px; }
</style>
