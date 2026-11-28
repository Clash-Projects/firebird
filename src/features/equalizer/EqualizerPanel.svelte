<script lang="ts">
  import { onMount } from 'svelte';
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import {
    CLARITY_PRESETS,
    EQ_FREQUENCIES,
    EQ_PRESETS,
    featuresStore,
  } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import WaveSlider from '$src/ui/components/WaveSlider.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    embedded?: boolean;
  }

  let { embedded = false }: Props = $props();

  let selectedTab = $state<'eq' | 'clarity'>('eq');

  const presetNames = Object.keys(EQ_PRESETS);
  const headphoneOptions = [
    { value: 'off', label: 'Off (No AutoEQ)' },
    { value: 'sony-wh-1000xm5', label: 'Sony WH-1000XM5' },
    { value: 'bose-qc-ultra', label: 'Bose QC Ultra' },
    { value: 'airpods-max', label: 'Apple AirPods Max' },
    { value: 'sennheiser-hd600', label: 'Sennheiser HD 600' },
    { value: 'ath-m50x', label: 'Audio-Technica ATH-M50x' },
    { value: 'dt-770', label: 'Beyerdynamic DT 770' },
  ];

  onMount(() => {
    if (!isTauriRuntime()) return;
    void featuresStore.refreshEqState();
  });

  function changeHeadphoneProfile(id: string) {
    featuresStore.headphoneProfile = id;
    settingsStore.saveStringPref('lw_headphone_profile', id);
    void ipcInvoke('player_apply_audio_prefs');
  }

  function fmtFreq(hz: number): string {
    return hz >= 1000 ? `${hz / 1000}k` : String(hz);
  }

  let activeClarity = $derived(
    CLARITY_PRESETS.find((p) => p.key === featuresStore.clarityPresetKey) ?? CLARITY_PRESETS[0]
  );

  let isProcessing = $derived(featuresStore.eqEnabled || featuresStore.studioClarity);

  let statusSubtitle = $derived.by(() => {
    if (!isProcessing) return 'Bypassed (Bit-Perfect output)';
    if (featuresStore.studioClarity && featuresStore.eqEnabled) {
      return `Studio Clarity + ${featuresStore.eqPreset}`;
    }
    if (featuresStore.studioClarity) {
      return `Studio Master Clarity (${activeClarity.displayName})`;
    }
    return `${featuresStore.eqPreset} preset active`;
  });

  // Smooth cubic bezier SVG path matching OG _EqCurvePainter (330x68 coordinate box)
  let curveGeometry = $derived.by(() => {
    const gains = featuresStore.combinedEqGainsDb;
    const w = 330;
    const h = 68;
    const zeroY = h / 2;
    const n = gains.length;
    if (n === 0) return { points: [], line: '', fill: '' };
    const pts = gains.map((g, i) => {
      const x = (i / (n - 1)) * w;
      const norm = Math.max(-1, Math.min(1, g / 8));
      const y = zeroY - norm * (h * 0.41);
      return { x, y };
    });
    let line = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i];
      const p1 = pts[i + 1];
      const cx = (p0.x + p1.x) / 2;
      line += ` C ${cx.toFixed(1)} ${p0.y.toFixed(1)}, ${cx.toFixed(1)} ${p1.y.toFixed(1)}, ${p1.x.toFixed(1)} ${p1.y.toFixed(1)}`;
    }
    const fill = `${line} L ${w} ${zeroY} L 0 ${zeroY} Z`;
    return { points: pts, line, fill };
  });
</script>

<WaveHaze
  level={embedded ? 'l1' : 'l2'}
  enableBlur={!embedded}
  dataLw="equalizer-panel"
  class={embedded
    ? 'flex w-full flex-col rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)]'
    : 'flex h-full w-[min(390px,100vw)] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]'}
>
  <!-- Header -->
  <div class="flex h-[52px] shrink-0 items-center gap-2.5 border-b border-[var(--lw-divider)] px-4">
    <WaveIcon
      name="eq"
      size={16}
      class={isProcessing ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}
    />
    <div class="min-w-0 flex-1">
      <h2 class="truncate text-[13.5px] font-semibold text-[var(--lw-text-primary)]">Equalizer &amp; Sound FX</h2>
      <p
        class="truncate text-[11px] {isProcessing
          ? featuresStore.studioClarity
            ? 'text-[#F5A623]'
            : 'text-[var(--lw-accent)]'
          : 'text-[var(--lw-text-tertiary)]'}"
      >
        {statusSubtitle}
      </p>
    </div>

    <Switch
      checked={featuresStore.eqEnabled}
      onchange={(v) => featuresStore.setEqEnabled(v)}
      label="Enable Equalizer"
    />

    <button
      type="button"
      use:waveTooltip={'Reset all bands to flat (0 dB)'}
      onclick={() => featuresStore.resetEq()}
      aria-label="Reset equalizer to flat"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="refresh" size={13} />
    </button>

    {#if !embedded}
      <button
        type="button"
        use:waveTooltip={'Close panel'}
        onclick={() => windowState.closeRightPanel()}
        aria-label="Close equalizer"
        class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name="close" size={14} />
      </button>
    {/if}
  </div>

  <!-- Tab Selector -->
  <div class="grid shrink-0 grid-cols-2 gap-2 border-b border-[var(--lw-divider)] px-4 py-2">
    <button
      type="button"
      onclick={() => (selectedTab = 'eq')}
      class="flex cursor-pointer items-center justify-center gap-1.5 rounded-[8px] border py-1.5 text-[11.5px] transition-all {selectedTab === 'eq'
        ? 'border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] font-semibold text-[var(--lw-text-primary)] shadow-[0_1px_4px_rgba(0,0,0,0.12)]'
        : 'border-transparent text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
    >
      <WaveIcon name="eq" size={13} class={featuresStore.eqEnabled ? 'text-[#F5A623]' : ''} />
      <span>10-Band EQ</span>
      {#if featuresStore.eqEnabled}
        <span class="h-1.5 w-1.5 rounded-full bg-[#F5A623]"></span>
      {/if}
    </button>

    <button
      type="button"
      onclick={() => (selectedTab = 'clarity')}
      class="flex cursor-pointer items-center justify-center gap-1.5 rounded-[8px] border py-1.5 text-[11.5px] transition-all {selectedTab === 'clarity'
        ? 'border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] font-semibold text-[var(--lw-text-primary)] shadow-[0_1px_4px_rgba(0,0,0,0.12)]'
        : 'border-transparent text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
    >
      <WaveIcon name="sparkles" size={13} class={featuresStore.studioClarity ? 'text-[#F5A623]' : ''} />
      <span>Studio Clarity</span>
      {#if featuresStore.studioClarity}
        <span class="h-1.5 w-1.5 rounded-full bg-[#F5A623]"></span>
      {/if}
    </button>
  </div>

  <!-- Body -->
  <div use:waveSmoothScroll class="lw-scroll flex-1 space-y-3.5 px-4 py-3">
    {#if selectedTab === 'eq'}
      <!-- Live Frequency Response Visualizer -->
      <div class="rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3 py-2">
        <svg viewBox="0 0 330 68" class="h-[68px] w-full overflow-visible" aria-label="EQ frequency response curve">
          <defs>
            <linearGradient id="lw-eq-curve-grad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color={isProcessing ? '#E5B760' : 'var(--lw-text-tertiary)'} stop-opacity={isProcessing ? '0.28' : '0.09'} />
              <stop offset="100%" stop-color={isProcessing ? '#E5B760' : 'var(--lw-text-tertiary)'} stop-opacity="0" />
            </linearGradient>
          </defs>
          <line x1="0" y1="34" x2="330" y2="34" stroke="var(--lw-divider)" stroke-width="1" />
          {#if curveGeometry.line}
            <path d={curveGeometry.fill} fill="url(#lw-eq-curve-grad)" />
            <path
              d={curveGeometry.line}
              fill="none"
              stroke={isProcessing ? '#E5B760' : 'var(--lw-text-tertiary)'}
              stroke-opacity={isProcessing ? '0.95' : '0.45'}
              stroke-width="2"
              stroke-linecap="round"
            />
            {#each curveGeometry.points as pt, i (i)}
              <circle
                cx={pt.x}
                cy={pt.y}
                r="2.4"
                fill={isProcessing ? '#E5B760' : 'var(--lw-text-tertiary)'}
                fill-opacity={isProcessing ? '1' : '0.5'}
              />
            {/each}
          {/if}
        </svg>
      </div>

      <!-- Horizontal Preset Pills -->
      <div use:waveSmoothScroll={{ axis: 'x', drag: true }} class="lw-scroll-x flex items-center gap-1.5 pb-0.5">
        {#each presetNames as name (name)}
          {@const selected = featuresStore.eqPreset.toLowerCase() === name.toLowerCase()}
          {@const isMaster = name === 'Studio Master'}
          <button
            type="button"
            onclick={() => featuresStore.applyEqPreset(name)}
            class="shrink-0 cursor-pointer rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors {selected
              ? isMaster
                ? 'border-transparent bg-[#8A6D3B] font-semibold text-white'
                : 'border-transparent bg-[var(--lw-text-primary)] font-semibold text-[var(--lw-bg)]'
              : 'border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
          >
            {name}
          </button>
        {/each}
      </div>

      <!-- 10-Band Vertical Slider Rack -->
      <div class="rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-2.5 py-3">
        <div class="relative grid grid-cols-10 gap-0.5">
          <!-- Center 0dB guide line behind sliders -->
          <div
            class="pointer-events-none absolute right-2 left-2 h-px bg-[var(--lw-divider)] opacity-70"
            style:top="88px"
            aria-hidden="true"
          ></div>
          {#each EQ_FREQUENCIES as hz, idx (hz)}
            {@const val = featuresStore.eqBands[idx] ?? 0}
            <div class="flex flex-col items-center gap-1.5">
              <span
                class="h-4 text-[9.5px] font-semibold lw-tabular {val !== 0 && featuresStore.eqEnabled
                  ? val > 0
                    ? 'text-[var(--lw-accent)]'
                    : 'text-[var(--lw-text-primary)]'
                  : 'text-[var(--lw-text-tertiary)]'}"
              >
                {val > 0 ? '+' : ''}{val.toFixed(1)}
              </span>

              <WaveSlider
                orientation="vertical"
                height={128}
                min={-8}
                max={8}
                step={0.5}
                disabled={!featuresStore.eqEnabled}
                label={`${fmtFreq(hz)} Hz gain`}
                valuetext={`${val > 0 ? '+' : ''}${val.toFixed(1)} dB`}
                value={val}
                oninput={(v) => featuresStore.setEqBand(idx, v)}
              />

              <button
                type="button"
                use:waveTooltip={'Double-click to reset band'}
                ondblclick={() => featuresStore.setEqBand(idx, 0)}
                class="cursor-pointer rounded-[4px] bg-[var(--lw-wash)] px-1 py-0.5 text-[9.5px] font-medium text-[var(--lw-text-secondary)] lw-tabular hover:text-[var(--lw-text-primary)]"
              >
                {fmtFreq(hz)}
              </button>
            </div>
          {/each}
        </div>
      </div>

      <!-- Pre-Amp & AutoEQ -->
      <div class="space-y-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3">
        <div>
          <div class="mb-1.5 flex items-center justify-between text-[12px]">
            <span class="text-[var(--lw-text-secondary)]">Pre-Amp Headroom</span>
            <span class="font-semibold text-[var(--lw-text-primary)] lw-tabular">
              {featuresStore.eqPreampDb > 0 ? '+' : ''}{featuresStore.eqPreampDb.toFixed(1)} dB
            </span>
          </div>
          <WaveSlider
            min={-12}
            max={6}
            step={0.5}
            label="Pre-Amp Headroom"
            valuetext={`${featuresStore.eqPreampDb.toFixed(1)} dB`}
            bind:value={featuresStore.eqPreampDb}
          />
        </div>

        <div class="border-t border-[var(--lw-divider)] pt-2.5">
          <div class="mb-1.5 text-[12px] font-medium text-[var(--lw-text-secondary)]">AutoEQ Headphone Curve</div>
          <Select
            label="AutoEQ Headphone Curve"
            value={featuresStore.headphoneProfile}
            options={headphoneOptions}
            onchange={changeHeadphoneProfile}
            class="w-full"
          />
        </div>
      </div>
    {:else}
      <!-- Tab 1: Studio Master Clarity -->
      <div
        class="clarity-hero rounded-[10px] border p-3.5 transition-colors"
        data-active={featuresStore.studioClarity}
      >
        <div class="flex items-center gap-3">
          <div class="clarity-icon-box flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]">
            <WaveIcon name="sparkles" size={17} />
          </div>
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-bold">Studio Master Clarity</div>
            <div class="clarity-sub text-[11px]">
              Psychoacoustic high-frequency restoration &amp; air staging
            </div>
          </div>
          <Switch
            checked={featuresStore.studioClarity}
            onchange={(v) => featuresStore.setStudioClarityEnabled(v)}
            label="Enable Studio Master Clarity"
          />
        </div>
      </div>

      <div>
        <div class="mb-2 text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          ACOUSTIC TARGET PROFILE
        </div>
        <div class="space-y-2">
          {#each CLARITY_PRESETS as target (target.key)}
            {@const selected = featuresStore.clarityPresetKey === target.key}
            <button
              type="button"
              onclick={() => featuresStore.setClarityPreset(target.key)}
              data-selected={selected}
              class="clarity-option flex w-full cursor-pointer items-center gap-3 rounded-[10px] border p-3 text-left transition-all"
            >
              <span
                class="h-4 w-4 shrink-0 rounded-full border-2 {selected
                  ? featuresStore.studioClarity
                    ? 'border-[#F5A623] bg-[#F5A623]'
                    : 'border-[#F5A623]'
                  : 'border-[var(--lw-text-tertiary)]'}"
              ></span>
              <div class="min-w-0 flex-1">
                <div class="clarity-opt-title text-[12.5px] font-semibold">{target.displayName}</div>
                <div class="clarity-opt-desc mt-0.5 text-[11px]">
                  {target.description}
                </div>
              </div>
            </button>
          {/each}
        </div>
      </div>

      <div class="rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3 py-2">
        <div class="mb-1 text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
          Combined Curve Preview
        </div>
        <svg viewBox="0 0 330 68" class="h-[68px] w-full overflow-visible">
          <line x1="0" y1="34" x2="330" y2="34" stroke="var(--lw-divider)" stroke-width="1" />
          {#if curveGeometry.line}
            <path d={curveGeometry.fill} fill="url(#lw-eq-curve-grad)" />
            <path
              d={curveGeometry.line}
              fill="none"
              stroke={featuresStore.studioClarity ? '#E5B760' : 'var(--lw-text-tertiary)'}
              stroke-opacity={featuresStore.studioClarity ? '0.95' : '0.45'}
              stroke-width="2"
              stroke-linecap="round"
            />
          {/if}
        </svg>
      </div>
    {/if}
  </div>
</WaveHaze>

<style>
  .clarity-hero {
    border-color: var(--lw-outline-soft);
    background: var(--lw-surface);
    color: var(--lw-text-primary);
  }
  .clarity-hero .clarity-sub {
    color: var(--lw-text-secondary);
  }
  .clarity-hero .clarity-icon-box {
    background: var(--lw-wash);
    color: var(--lw-text-secondary);
  }
  .clarity-hero[data-active='true'] {
    border-color: rgba(138, 109, 59, 0.8);
    background: #1f1b14;
    color: #ffffff;
  }
  .clarity-hero[data-active='true'] .clarity-sub {
    color: rgba(255, 255, 255, 0.75);
  }
  .clarity-hero[data-active='true'] .clarity-icon-box {
    background: rgba(245, 166, 35, 0.2);
    color: #f5a623;
  }
  :global(:root[data-theme='light']) .clarity-hero[data-active='true'] {
    border-color: #e5b760;
    background: #fff9ed;
    color: #1b1a19;
  }
  :global(:root[data-theme='light']) .clarity-hero[data-active='true'] .clarity-sub {
    color: #605e5c;
  }

  .clarity-option {
    border-color: var(--lw-outline-soft);
    background: var(--lw-surface);
    color: var(--lw-text-primary);
  }
  .clarity-option:hover {
    background: var(--lw-wash-hover);
  }
  .clarity-option .clarity-opt-desc {
    color: var(--lw-text-secondary);
  }
  .clarity-option[data-selected='true'] {
    border-color: #f5a623;
    background: #2b2418;
    color: #ffffff;
  }
  .clarity-option[data-selected='true'] .clarity-opt-desc {
    color: rgba(255, 255, 255, 0.75);
  }
  :global(:root[data-theme='light']) .clarity-option[data-selected='true'] {
    border-color: #e5b760;
    background: #fff3d6;
    color: #5a3e00;
  }
  :global(:root[data-theme='light']) .clarity-option[data-selected='true'] .clarity-opt-desc {
    color: #605e5c;
  }
</style>
