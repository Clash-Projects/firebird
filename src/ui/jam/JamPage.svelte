<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, ipcListen, isTauriRuntime } from '$src/lib/ipc';
  import { router } from '$src/app/router.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  type JamPhase = 'disconnected' | 'connecting' | 'active' | 'error';

  interface JamPerson {
    userId: string;
    name: string;
    isHost: boolean;
    isCoHost: boolean;
    isWeb: boolean;
  }

  interface JamReply {
    id: string;
    userName: string;
    text: string;
  }

  interface JamChatLine {
    id: string;
    userId: string;
    userName: string;
    isHost: boolean;
    text: string;
    timestamp: number;
    replyTo?: JamReply | null;
  }

  interface JamTrack {
    id: string;
    title: string;
    artist: string;
    album?: string | null;
    artwork_url?: string | null;
    duration_ms?: number | null;
  }

  interface JamSnapshot {
    phase: JamPhase;
    error: string;
    roomCode: string;
    userId: string;
    displayName: string;
    isHost: boolean;
    hostId: string;
    coHostId: string;
    permanent: boolean;
    radio: boolean;
    stationAuto: boolean;
    roomMode: string;
    people: JamPerson[];
    chat: JamChatLine[];
    track?: JamTrack | null;
    isPlaying: boolean;
    queue: JamTrack[];
    roomName: string;
    cooldownUntilMs: number;
    notice: string;
    retryAtMs: number;
    webGuests: boolean;
  }

  const JAM_STATIONS = [
    { code: 'INDIA', title: 'India', blurb: 'Always-on room for India' },
    { code: 'WORLD', title: 'International', blurb: 'Always-on room for everyone else' },
  ] as const;

  const BROWSER_GUEST_WARN_AT = 8;

  let snap = $state<JamSnapshot>({
    phase: 'disconnected',
    error: '',
    roomCode: '',
    userId: '',
    displayName: '',
    isHost: false,
    hostId: '',
    coHostId: '',
    permanent: false,
    radio: false,
    stationAuto: true,
    roomMode: 'collaborative',
    people: [],
    chat: [],
    track: null,
    isPlaying: false,
    queue: [],
    roomName: '',
    cooldownUntilMs: 0,
    notice: '',
    retryAtMs: 0,
    webGuests: false,
  });

  let nameInput = $state(settingsStore.jamDisplayName || '');
  let codeInput = $state('');
  let chatInput = $state('');
  let collaborative = $state(true);
  let allowBrowser = $state(true);
  let copied = $state(false);
  let counts = $state<Record<string, number>>({});
  let replyTo = $state<JamChatLine | null>(null);
  let nowTick = $state(Date.now());
  let chatScrollEl = $state<HTMLDivElement | null>(null);

  let active = $derived(snap.phase === 'active');
  let busy = $derived(snap.phase === 'connecting');
  let isMainHost = $derived(Boolean(snap.userId) && snap.userId === snap.hostId);
  let webGuestCount = $derived(snap.people.filter((p) => p.isWeb).length);
  let sharedQueue = $derived(
    snap.queue.length > 0
      ? snap.queue
      : snap.isHost && !snap.radio
        ? playerStore.queue
            .slice(playerStore.currentIndex + 1)
            .map((t) => ({
              id: t.videoId || t.id,
              title: t.title,
              artist: t.artist,
              album: t.album,
              artwork_url: t.artworkUrl,
              duration_ms: t.durationSec ? t.durationSec * 1000 : null,
            }))
        : []
  );

  function syncFeaturesStore(s: JamSnapshot) {
    featuresStore.jamConnected = s.phase === 'active' && Boolean(s.roomCode);
    featuresStore.jamRoomCode = s.roomCode || '';
    featuresStore.jamIsHost = s.isHost;
    featuresStore.jamParticipants = s.people.map((p) => ({
      id: p.userId,
      name: p.name,
      role: p.userId === s.hostId ? 'host' : p.isCoHost ? 'co-host' : p.isWeb ? 'browser' : 'listener',
    }));
    featuresStore.jamChat = s.chat.map((c) => ({
      id: c.id,
      sender: c.userName,
      text: c.text,
      time: formatClock(c.timestamp),
    }));
  }

  function applySnapshot(next: JamSnapshot | null) {
    if (!next) return;
    const prevLen = snap.chat.length;
    snap = next;
    syncFeaturesStore(next);
    if (replyTo && !next.chat.some((c) => c.id === replyTo?.id)) {
      replyTo = null;
    }
    if (next.chat.length !== prevLen) {
      queueMicrotask(() => {
        if (chatScrollEl) {
          chatScrollEl.scrollTop = chatScrollEl.scrollHeight;
        }
      });
    }
  }

  async function refreshSnapshot() {
    if (!isTauriRuntime()) return;
    const res = await ipcInvoke<JamSnapshot>('jam_snapshot');
    applySnapshot(res);
  }

  async function refreshCounts() {
    if (!isTauriRuntime()) return;
    const res = await ipcInvoke<Record<string, number>>('jam_room_counts');
    if (res) counts = res;
  }

  function saveNamePref(val: string) {
    nameInput = val;
    const trimmed = val.trim();
    settingsStore.jamDisplayName = trimmed;
    settingsStore.saveStringPref('lw_jam_display_name', trimmed);
  }

  function resolvedName(): string {
    const typed = nameInput.trim();
    if (typed) return typed;
    if (settingsStore.jamDisplayName.trim()) return settingsStore.jamDisplayName.trim();
    if (settingsStore.displayName.trim()) return settingsStore.displayName.trim();
    if (settingsStore.ytAccountName.trim()) return settingsStore.ytAccountName.trim();
    return '';
  }

  function formatClock(ms: number): string {
    if (!ms) return '';
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return '';
    const h24 = d.getHours();
    const h = h24 % 12 === 0 ? 12 : h24 % 12;
    const m = d.getMinutes().toString().padStart(2, '0');
    const s = d.getSeconds().toString().padStart(2, '0');
    return `${h}:${m}:${s} ${h24 < 12 ? 'AM' : 'PM'}`;
  }

  function formatNoticeText(s: JamSnapshot): string {
    if (!s.notice) return '';
    if (s.retryAtMs > 0) {
      return `One pause or change per minute. Try again at ${formatClock(s.retryAtMs)}.`;
    }
    return s.notice;
  }

  function avatarInitial(name: string): string {
    const trimmed = name.trim();
    return trimmed ? trimmed[0].toUpperCase() : '?';
  }

  function startRoom() {
    if (busy) return;
    const name = resolvedName() || 'Listener';
    saveNamePref(name);
    void ipcInvoke('jam_create_room', {
      displayName: name,
      collaborative,
      browserGuests: allowBrowser,
    });
  }

  function joinRoom(rawCode?: string) {
    if (busy) return;
    const code = (rawCode ?? codeInput).trim();
    const name = resolvedName() || 'Listener';
    if (nameInput.trim()) saveNamePref(nameInput.trim());
    void ipcInvoke('jam_join_room', {
      roomCode: code,
      displayName: name,
    });
  }

  function leaveRoom() {
    replyTo = null;
    void ipcInvoke('jam_leave');
  }

  async function copyInvite() {
    const link = await ipcInvoke<string>('jam_invite_link', { code: snap.roomCode });
    const text = link || snap.roomCode;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => {
        copied = false;
      }, 2000);
    } catch {
      featuresStore.showToast({ title: 'Could not copy invite link', severity: 'error' });
    }
  }

  function sendChat() {
    const text = chatInput.trim();
    if (!text) return;
    void ipcInvoke('jam_send_chat', {
      text,
      replyToId: replyTo?.id ?? null,
    });
    chatInput = '';
    replyTo = null;
  }

  function dismissNotice() {
    void ipcInvoke('jam_clear_notice');
  }

  onMount(() => {
    if (!nameInput && settingsStore.jamDisplayName) {
      nameInput = settingsStore.jamDisplayName;
    }
    void refreshSnapshot();
    void refreshCounts();

    let unlisten: (() => void) | null = null;
    void ipcListen<JamSnapshot>('jam:snapshot', (payload) => {
      applySnapshot(payload);
    }).then((fn) => {
      unlisten = fn;
    });

    const countsTimer = setInterval(() => {
      if (snap.phase !== 'active') {
        void refreshCounts();
      }
    }, 15000);

    const tickTimer = setInterval(() => {
      nowTick = Date.now();
      if (snap.phase === 'connecting') {
        void refreshSnapshot();
      }
      if (snap.retryAtMs > 0 && nowTick >= snap.retryAtMs + 500) {
        void ipcInvoke('jam_clear_notice');
      }
    }, 1000);

    return () => {
      unlisten?.();
      clearInterval(countsTimer);
      clearInterval(tickTimer);
    };
  });
</script>

<div class="flex h-full flex-col px-7 pt-6 pb-6">
  <!-- Header matching jam_page.dart -->
  <div class="flex items-center gap-3">
    <h1 class="text-[28px] font-bold tracking-[-0.6px] text-[var(--lw-text-primary)]">Jam</h1>
    {#if active}
      <span class="inline-flex items-center gap-1.5 rounded-full bg-[color-mix(in_srgb,var(--lw-success)_14%,transparent)] px-2.5 py-0.5 text-[11px] font-semibold text-[var(--lw-success)]">
        <span class="h-1.5 w-1.5 rounded-full bg-[var(--lw-success)]"></span>
        Live
      </span>
    {/if}
    <div class="flex-1"></div>
    <button
      type="button"
      use:waveTooltip={'Close'}
      aria-label="Close Jam"
      onclick={() => router.navigate('/home')}
      class="flex h-8 w-8 items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] transition hover:bg-[var(--lw-surface-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="close" size={14} />
    </button>
  </div>

  <p class="mt-1.5 text-[13px] text-[var(--lw-text-secondary)]">
    {active
      ? 'Playback stays in sync with everyone in the room.'
      : 'Listen together. Rooms run on a hosted Clash Projects server; if it is unreachable, playback on this device keeps going.'}
  </p>

  {#if !active}
    <!-- Entry Screen (1:1 single 460px column of Wave cards) -->
    <div class="lw-scroll mt-5 flex-1 overflow-y-auto pr-1 pb-8">
      <div class="flex max-w-[460px] flex-col gap-3.5">
        {#if snap.phase === 'error'}
          <div class="flex items-start gap-2.5 rounded-[10px] border border-[color-mix(in_srgb,var(--lw-warning)_40%,transparent)] bg-[color-mix(in_srgb,var(--lw-warning)_12%,transparent)] px-3.5 py-2.5 text-[12.5px] text-[var(--lw-text-primary)]">
            <WaveIcon name="info" size={16} class="mt-0.5 shrink-0 text-[var(--lw-warning)]" />
            <span>{snap.error || 'Connection failed'}</span>
          </div>
        {/if}

        <!-- Your name -->
        <div class="jam-card">
          <div class="jam-label">YOUR NAME</div>
          <div class="mt-1.5">
            <Input
              value={nameInput}
              placeholder="Listener"
              oninput={(val) => saveNamePref(val)}
            />
          </div>
        </div>

        <!-- Always-on rooms -->
        <div class="jam-card">
          <h2 class="text-[15px] font-bold text-[var(--lw-text-primary)]">Always-on rooms</h2>
          <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
            Open all day. The first person in is the admin.
          </p>
          <div class="mt-3 space-y-2">
            {#each JAM_STATIONS as station (station.code)}
              {@const count = counts[station.code]}
              <div class="flex items-center gap-2 py-1">
                <div class="min-w-0 flex-1">
                  <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">
                    {station.title}
                  </div>
                  <div class="text-[12px] text-[var(--lw-text-secondary)]">
                    {typeof count === 'number'
                      ? `${count} listening · ${station.code}`
                      : `${station.blurb} · ${station.code}`}
                  </div>
                </div>
                <Buttons
                  variant="primary"
                  size="sm"
                  disabled={busy}
                  onclick={() => joinRoom(station.code)}
                >
                  Join
                </Buttons>
              </div>
            {/each}
          </div>
        </div>

        <!-- Start a room -->
        <div class="jam-card">
          <h2 class="text-[15px] font-bold text-[var(--lw-text-primary)]">Start a room</h2>
          <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
            You get a code to share with friends.
          </p>

          <div class="mt-3.5 space-y-2.5">
            <div class="flex items-center justify-between gap-3">
              <span class="text-[13px] text-[var(--lw-text-primary)]">
                {collaborative ? 'Everyone controls playback' : 'Only the host controls playback'}
              </span>
              <Switch bind:checked={collaborative} label="Collaborative playback" />
            </div>

            <div class="flex items-center justify-between gap-3">
              <span class="text-[13px] text-[var(--lw-text-primary)]">Allow browser guests</span>
              <Switch bind:checked={allowBrowser} label="Allow browser guests" />
            </div>
          </div>

          <p class="mt-2 text-[11.5px] leading-relaxed text-[var(--lw-text-secondary)]">
            Your invite link also plays in any browser, no app needed. They hear the YouTube version (~128 kbps) streamed from this PC.
          </p>

          <div class="mt-3.5">
            <Buttons variant="primary" disabled={busy} onclick={startRoom}>
              {busy ? 'Connecting…' : 'Create room'}
            </Buttons>
          </div>
        </div>

        <!-- Join a room -->
        <div class="jam-card">
          <h2 class="text-[15px] font-bold text-[var(--lw-text-primary)]">Join a room</h2>
          <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
            Enter the code a friend sent you.
          </p>
          <div class="mt-3 flex items-center gap-2">
            <div class="flex-1">
              <Input
                bind:value={codeInput}
                placeholder="ABC123"
                class="font-semibold tracking-[3px]"
                oninput={(val) => {
                  if (!val.includes('http') && !val.includes('code=')) {
                    codeInput = val.toUpperCase();
                  }
                }}
                onkeydown={(e) => {
                  if (e.key === 'Enter' && !busy) {
                    e.preventDefault();
                    joinRoom();
                  }
                }}
              />
            </div>
            <Buttons variant="secondary" disabled={busy} onclick={() => joinRoom()}>
              Join
            </Buttons>
          </div>
        </div>
      </div>
    </div>
  {:else}
    <!-- Active Room (1:1 340px left info column + flexible right chat card) -->
    {#if snap.notice}
      <div class="mt-4 flex items-center justify-between gap-3 rounded-[10px] border border-[color-mix(in_srgb,var(--lw-warning)_40%,transparent)] bg-[color-mix(in_srgb,var(--lw-warning)_12%,transparent)] px-3.5 py-2 text-[12.5px] text-[var(--lw-text-primary)]">
        <div class="flex items-center gap-2.5">
          <WaveIcon name="info" size={15} class="shrink-0 text-[var(--lw-warning)]" />
          <span>{formatNoticeText(snap)}</span>
        </div>
        <button
          type="button"
          use:waveTooltip={'Dismiss'}
          aria-label="Dismiss notice"
          onclick={dismissNotice}
          class="flex h-6 w-6 items-center justify-center rounded text-[var(--lw-text-secondary)] hover:bg-[var(--lw-surface-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    {/if}

    <div class="mt-5 flex min-h-0 flex-1 flex-col gap-4 lg:flex-row">
      <!-- Left 340px column -->
      <div class="lw-scroll flex w-full shrink-0 flex-col gap-3.5 overflow-y-auto pr-1 pb-6 lg:w-[340px]">
        <!-- Room code card -->
        <div class="jam-card">
          <div class="jam-label">
            {snap.roomName ? `Room · ${snap.roomCode}` : 'Room code'}
          </div>
          <div
            class="mt-2 select-text rounded-[10px] border px-3.5 py-3 text-center font-bold text-[var(--lw-text-primary)]"
            style="background: color-mix(in srgb, var(--lw-accent) 10%, transparent); border-color: color-mix(in srgb, var(--lw-accent) 28%, transparent); font-size: 30px; letter-spacing: {snap.roomName ? '1px' : '8px'};"
          >
            {snap.roomName || snap.roomCode}
          </div>

          <div class="mt-2.5 flex flex-wrap gap-1.5">
            <span class="jam-pill">
              {#if snap.radio}
                {snap.isHost ? 'Station admin' : 'Listener'}
              {:else if isMainHost}
                Host
              {:else if snap.isHost}
                Co-host
              {:else}
                Guest
              {/if}
            </span>

            {#if snap.radio}
              <span class="jam-pill" style="color: var(--lw-success); background: color-mix(in srgb, var(--lw-success) 14%, transparent);">
                <span class="h-1.5 w-1.5 rounded-full bg-[var(--lw-success)]"></span>
                Live station
              </span>
            {:else if snap.permanent}
              <span class="jam-pill">Always on</span>
            {/if}

            <span class="jam-pill">
              {#if snap.radio}
                {snap.stationAuto ? 'Auto radio' : 'Admin controls playback'}
              {:else if snap.roomMode === 'host_only'}
                Host controls playback
              {:else}
                Everyone controls playback
              {/if}
            </span>
          </div>

          <div class="mt-3.5 flex items-center gap-2">
            <div class="flex-1">
              <Buttons variant="primary" class="w-full" onclick={copyInvite}>
                {copied ? 'Copied' : 'Copy invite'}
              </Buttons>
            </div>
            <Buttons variant="secondary" onclick={leaveRoom}>
              Leave
            </Buttons>
          </div>
        </div>

        <!-- Browser guests card (main host + enabled) -->
        {#if isMainHost && snap.webGuests}
          <div class="jam-card">
            <div class="jam-label">Browser guests · {webGuestCount} listening</div>
            <p class="mt-1.5 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
              The invite link plays in any browser. Guests get the YouTube version (~128 kbps) straight from this PC.
            </p>
            {#if webGuestCount >= BROWSER_GUEST_WARN_AT}
              <div class="mt-2 flex items-start gap-2 rounded-[10px] border border-[color-mix(in_srgb,var(--lw-warning)_40%,transparent)] bg-[color-mix(in_srgb,var(--lw-warning)_12%,transparent)] px-3 py-2 text-[12px] text-[var(--lw-text-primary)]">
                <WaveIcon name="info" size={14} class="mt-0.5 shrink-0 text-[var(--lw-warning)]" />
                <span>
                  {webGuestCount} browser guests use about {webGuestCount * 4} MB of your upload per song. If they stutter, ask some to get the app.
                </span>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Now playing -->
        <div class="jam-card">
          <div class="jam-label">Now playing</div>
          {#if !snap.track}
            <p class="mt-2.5 text-[12px] text-[var(--lw-text-secondary)]">
              Nothing yet. Play a song and it plays for everyone.
            </p>
          {:else}
            <div class="mt-2.5 flex items-center gap-3.5">
              <Artwork src={snap.track.artwork_url || ''} size={64} radius={10} />
              <div class="min-w-0 flex-1">
                <div class="line-clamp-2 text-[14px] font-semibold text-[var(--lw-text-primary)]">
                  {snap.track.title}
                </div>
                <div class="mt-0.5 truncate text-[12px] text-[var(--lw-text-secondary)]">
                  {snap.track.artist}
                </div>
              </div>
            </div>
          {/if}
        </div>

        <!-- Up next -->
        <div class="jam-card">
          <div class="jam-label">
            {snap.radio && snap.stationAuto
              ? 'Up next on the station'
              : isMainHost
                ? 'Up next · your queue is shared'
                : 'Up next · admin queue'}
          </div>
          {#if sharedQueue.length === 0}
            <p class="mt-2.5 text-[12px] text-[var(--lw-text-secondary)]">Nothing queued.</p>
          {:else}
            <div class="mt-2.5 space-y-1.5">
              {#each sharedQueue.slice(0, 5) as qt, i (`${qt.id}-${i}`)}
                <div class="flex items-center text-[13px]">
                  <span class="w-[22px] shrink-0 text-[12px] text-[var(--lw-text-secondary)] lw-tabular">
                    {i + 1}
                  </span>
                  <span class="min-w-0 flex-1 truncate text-[var(--lw-text-primary)]">
                    {qt.artist ? `${qt.title} · ${qt.artist}` : qt.title}
                  </span>
                </div>
              {/each}
              {#if sharedQueue.length > 5}
                <div class="pt-1 text-[12px] text-[var(--lw-text-secondary)]">
                  +{sharedQueue.length - 5} more
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Listeners -->
        <div class="jam-card">
          <div class="jam-label">Listeners · {snap.people.length}</div>
          <div class="mt-2.5 space-y-2">
            {#each snap.people as person (person.userId)}
              {@const isSelf = person.userId === snap.userId}
              <div class="flex items-center gap-2.5 py-0.5">
                <div
                  class="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-full text-[12.5px] font-bold text-[var(--lw-text-primary)]"
                  style="background: color-mix(in srgb, var(--lw-accent) 18%, transparent);"
                >
                  {avatarInitial(person.name)}
                </div>
                <span class="min-w-0 flex-1 truncate text-[13px] text-[var(--lw-text-primary)]">
                  {isSelf ? `${person.name} (you)` : person.name}
                </span>
                {#if person.isWeb}
                  <span class="jam-pill">Browser</span>
                {/if}
                {#if person.isHost}
                  <span class="jam-pill">
                    {snap.radio ? 'Admin' : person.isCoHost ? 'Co-host' : 'Host'}
                  </span>
                {/if}

                {#if snap.radio && isMainHost && !isSelf}
                  <Buttons
                    variant="secondary"
                    size="sm"
                    onclick={() => void ipcInvoke('jam_transfer_admin', { userId: person.userId })}
                  >
                    Make admin
                  </Buttons>
                {/if}

                {#if !snap.radio && isMainHost && !person.isWeb && !isSelf}
                  {#if person.isCoHost}
                    <Buttons
                      variant="secondary"
                      size="sm"
                      onclick={() => void ipcInvoke('jam_revoke_host', { userId: person.userId })}
                    >
                      Remove host
                    </Buttons>
                  {:else if !snap.coHostId}
                    <Buttons
                      variant="secondary"
                      size="sm"
                      onclick={() => void ipcInvoke('jam_grant_host', { userId: person.userId })}
                    >
                      Make host
                    </Buttons>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        </div>
      </div>

      <!-- Right Chat Card -->
      <div class="jam-card flex min-h-[420px] flex-1 flex-col !p-0">
        <div class="px-4 pt-3.5 pb-2.5">
          <div class="jam-label">Chat</div>
        </div>
        <div class="h-px bg-[var(--lw-outline)]"></div>

        <div bind:this={chatScrollEl} class="lw-scroll flex-1 overflow-y-auto p-4">
          {#if snap.chat.length === 0}
            <div class="flex h-full items-center justify-center text-[12px] text-[var(--lw-text-tertiary)]">
              No messages yet. Say hi.
            </div>
          {:else}
            <div class="flex flex-col">
              {#each snap.chat as line, i (line.id)}
                {@const mine = line.userId === snap.userId}
                {@const prev = i > 0 ? snap.chat[i - 1] : null}
                {@const showName = !prev || prev.userId !== line.userId}
                <div
                  class="flex flex-col {mine ? 'items-end' : 'items-start'}"
                  style="margin-top: {i === 0 ? 0 : showName ? 10 : 3}px;"
                >
                  {#if showName && !mine}
                    <div class="mb-1 pl-1 text-[11.5px] font-semibold text-[var(--lw-text-secondary)]">
                      {line.isHost ? `${line.userName} · host` : line.userName}
                    </div>
                  {/if}
                  <div class="group flex max-w-[85%] items-center gap-1">
                    {#if mine}
                      <button
                        type="button"
                        use:waveTooltip={'Reply'}
                        aria-label="Reply"
                        onclick={() => (replyTo = line)}
                        class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-[var(--lw-text-tertiary)] opacity-0 transition group-hover:opacity-100 hover:bg-[var(--lw-surface-hover)] hover:text-[var(--lw-text-primary)]"
                      >
                        <WaveIcon name="arrowLeft" size={12} />
                      </button>
                    {/if}

                    <div
                      class="max-w-[420px] px-3 py-2 text-[13px] leading-snug text-[var(--lw-text-primary)] select-text"
                      style="
                        background: {mine ? 'color-mix(in srgb, var(--lw-accent) 18%, transparent)' : 'var(--lw-surface-raised)'};
                        border-radius: 14px 14px {mine ? '4px 14px' : '14px 4px'};
                      "
                    >
                      {#if line.replyTo}
                        <div
                          class="mb-1.5 rounded-[4px] border-l-[3px] px-2 py-1 text-[11.5px]"
                          style="background: rgba(0, 0, 0, 0.14); border-left-color: var(--lw-accent);"
                        >
                          <div class="font-semibold text-[var(--lw-accent)]">{line.replyTo.userName}</div>
                          <div class="line-clamp-2 text-[var(--lw-text-secondary)]">{line.replyTo.text}</div>
                        </div>
                      {/if}
                      <div class="break-words">{line.text}</div>
                    </div>

                    {#if !mine}
                      <button
                        type="button"
                        use:waveTooltip={'Reply'}
                        aria-label="Reply"
                        onclick={() => (replyTo = line)}
                        class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-[var(--lw-text-tertiary)] opacity-0 transition group-hover:opacity-100 hover:bg-[var(--lw-surface-hover)] hover:text-[var(--lw-text-primary)]"
                      >
                        <WaveIcon name="arrowLeft" size={12} />
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="h-px bg-[var(--lw-outline)]"></div>

        {#if replyTo}
          <div
            class="flex items-center gap-2 border-l-[3px] py-2 pr-2 pl-3.5 text-[12px]"
            style="background: color-mix(in srgb, var(--lw-accent) 8%, transparent); border-left-color: var(--lw-accent);"
          >
            <div class="min-w-0 flex-1">
              <div class="font-semibold text-[var(--lw-accent)]">Replying to {replyTo.userName}</div>
              <div class="truncate text-[var(--lw-text-secondary)]">{replyTo.text}</div>
            </div>
            <button
              type="button"
              use:waveTooltip={'Cancel reply'}
              aria-label="Cancel reply"
              onclick={() => (replyTo = null)}
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-[var(--lw-text-secondary)] hover:bg-[var(--lw-surface-hover)] hover:text-[var(--lw-text-primary)]"
            >
              <WaveIcon name="close" size={11} />
            </button>
          </div>
        {/if}

        <div class="flex items-center gap-2 p-3">
          <div class="flex-1">
            <Input
              bind:value={chatInput}
              placeholder="Message"
              onkeydown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  sendChat();
                }
              }}
            />
          </div>
          <Buttons variant="primary" onclick={sendChat}>Send</Buttons>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .jam-card {
    padding: 16px;
    border-radius: 16px;
    border: 1px solid var(--lw-outline);
    background: linear-gradient(
      to bottom,
      color-mix(in srgb, var(--lw-text-primary) 2.5%, var(--lw-surface)),
      var(--lw-surface)
    );
  }

  .jam-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.9px;
    text-transform: uppercase;
    color: var(--lw-text-tertiary);
  }

  .jam-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    color: var(--lw-text-secondary);
    background: color-mix(in srgb, var(--lw-text-secondary) 14%, transparent);
  }
</style>
