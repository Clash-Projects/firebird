<script lang="ts">
  import { onMount } from 'svelte';
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { EQ_FREQUENCIES, EQ_PRESETS, featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  const presetOptions = Object.keys(EQ_PRESETS).map((k) => ({ value: k, label: k }));
  const headphoneOptions = [
    { value: 'Off', label: 'Off (No AutoEQ)' },
    { value: 'Sennheiser HD 600', label: 'Sennheiser HD 600' },
    { value: 'Sennheiser HD 650', label: 'Sennheiser HD 650' },
    { value: 'HiFiMAN Sundara', label: 'HiFiMAN Sundara' },
    { value: 'Sony WH-1000XM5', label: 'Sony WH-1000XM5' },
    { value: 'Apple AirPods Max', label: 'Apple AirPods Max' },
  ];

  onMount(async () => {
    if (!isTauriRuntime()) return;
    const eq = await ipcInvoke<{ enabled?: boolean; studioClarityEnabled?: boolean; bandGains?: number[] }>(
      'eq_get_state'
    );
    if (!eq) return;
    if (typeof eq.enabled === 'boolean') featuresStore.eqEnabled = eq.enabled;
    if (typeof eq.studioClarityEnabled === 'boolean') featuresStore.studioClarity = eq.studioClarityEnabled;
    if (Array.isArray(eq.bandGains) && eq.bandGains.length === featuresStore.eqBands.length) {
      featuresStore.eqBands = [...eq.bandGains];
    }
  });

  function toggleEqEnabled(enabled: boolean) {
    featuresStore.eqEnabled = enabled;
    void ipcInvoke('eq_set_enabled', { enabled });
  }

  function toggleStudioClarity(enabled: boolean) {
    featuresStore.studioClarity = enabled;
    void ipcInvoke('eq_set_studio_clarity_enabled', { enabled });
  }

  function resetEq() {
    featuresStore.applyEqPreset('Flat');
    void ipcInvoke('eq_reset');
  }

  function fmtFreq(hz: number): string {
    return hz >= 1000 ? `${hz / 1000}k` : String(hz);
  }
</script>

<WaveHaze
  level="l2"
  dataLw="equalizer-panel"
  class="flex h-full w-[380px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] p-4 shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between pb-3">
    <div>
      <h2 class="text-[14px] font-bold">15-Band Parametric EQ</h2>
      <p class="text-[11.5px] text-[var(--lw-text-tertiary)]">Studio DSP filter chain</p>
    </div>
    <div class="flex items-center gap-2">
      <input
        type="checkbox"
        checked={featuresStore.eqEnabled}
        onchange={(e) => toggleEqEnabled(e.currentTarget.checked)}
        aria-label="Enable EQ"
        class="h-4 w-4 accent-[var(--lw-accent)]"
      />
      <button
        type="button"
        onclick={() => windowState.closeRightPanel()}
        class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="close" size={15} />
      </button>
    </div>
  </div>

  <div class="lw-scroll flex-1 space-y-4">
    <div class="grid grid-cols-2 gap-2">
      <Select
        value={featuresStore.eqPreset}
        options={[...presetOptions, { value: 'Custom', label: 'Custom' }]}
        onchange={(v) => v !== 'Custom' && featuresStore.applyEqPreset(v)}
      />
      <Buttons variant="secondary" size="sm" onclick={resetEq}>Reset Flat</Buttons>
    </div>

    <div class="rounded-[16px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3">
      <div class="flex items-center justify-between text-[12px]">
        <span class="text-[var(--lw-text-secondary)]">Pre-Amp Headroom</span>
        <span class="font-semibold lw-tabular">{featuresStore.eqPreampDb.toFixed(1)} dB</span>
      </div>
      <input
        type="range"
        min="-12"
        max="6"
        step="0.5"
        bind:value={featuresStore.eqPreampDb}
        class="mt-2 h-1.5 w-full cursor-pointer accent-[var(--lw-accent)]"
      />
    </div>

    <div class="rounded-[16px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3">
      <div class="space-y-2">
        {#each EQ_FREQUENCIES as hz, idx (hz)}
          <div class="flex items-center gap-2.5 text-[11.5px]">
            <span class="w-9 text-right text-[var(--lw-text-tertiary)] lw-tabular">{fmtFreq(hz)}</span>
            <input
              type="range"
              min="-12"
              max="12"
              step="0.5"
              disabled={!featuresStore.eqEnabled}
              value={featuresStore.eqBands[idx]}
              oninput={(e) => featuresStore.setEqBand(idx, Number((e.currentTarget as HTMLInputElement).value))}
              class="h-1 flex-1 cursor-pointer accent-[var(--lw-accent)] disabled:opacity-40"
            />
            <span class="w-11 text-right lw-tabular {featuresStore.eqBands[idx] !== 0 ? 'text-[var(--lw-text-primary)]' : 'text-[var(--lw-text-tertiary)]'}">
              {featuresStore.eqBands[idx] > 0 ? '+' : ''}{featuresStore.eqBands[idx].toFixed(1)}
            </span>
          </div>
        {/each}
      </div>
    </div>

    <div class="space-y-2 rounded-[16px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3">
      <div class="text-[12px] font-semibold">AutoEQ Headphone Curve</div>
      <Select bind:value={featuresStore.headphoneProfile} options={headphoneOptions} class="w-full" />
      <label class="flex cursor-pointer items-center justify-between pt-1 text-[12px]">
        <span class="text-[var(--lw-text-secondary)]">Studio Master Clarity Filter</span>
        <input
          type="checkbox"
          checked={featuresStore.studioClarity}
          onchange={(e) => toggleStudioClarity(e.currentTarget.checked)}
          class="h-4 w-4 accent-[var(--lw-accent)]"
        />
      </label>
    </div>
  </div>
</WaveHaze>
