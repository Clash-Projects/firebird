<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let chatInput = $state('');
  let joinCode = $state('');
  let busy = $state(false);

  interface BackendJamSnapshot {
    phase?: string;
    roomCode?: string;
    isHost?: boolean;
    people?: Array<{ id?: string; displayName?: string; name?: string; role?: string }>;
    chat?: Array<{ id?: string; sender?: string; displayName?: string; text?: string; time?: string }>;
  }

  function applyJamSnapshot(snap: BackendJamSnapshot | null) {
    if (!snap) return;
    featuresStore.jamConnected = Boolean(snap.roomCode) && snap.phase !== 'idle' && snap.phase !== 'disconnected';
    featuresStore.jamRoomCode = snap.roomCode || '';
    featuresStore.jamIsHost = Boolean(snap.isHost);
    if (Array.isArray(snap.people)) {
      featuresStore.jamParticipants = snap.people.map((p, idx) => ({
        id: p.id || `p-${idx}`,
        name: p.displayName || p.name || 'Listener',
        role: p.role || 'listener',
      }));
    }
    if (Array.isArray(snap.chat)) {
      featuresStore.jamChat = snap.chat.map((c, idx) => ({
        id: c.id || `c-${idx}`,
        sender: c.displayName || c.sender || 'Guest',
        text: c.text || '',
        time: c.time || '',
      }));
    }
  }

  async function refreshSnapshot() {
    if (!isTauriRuntime()) return;
    const snap = await ipcInvoke<BackendJamSnapshot>('jam_snapshot');
    applyJamSnapshot(snap);
  }

  onMount(() => {
    void refreshSnapshot();
    const timer = setInterval(() => {
      if (featuresStore.jamConnected) void refreshSnapshot();
    }, 3000);
    return () => clearInterval(timer);
  });

  function sendChat(e: SubmitEvent) {
    e.preventDefault();
    if (!chatInput.trim()) return;
    featuresStore.sendJamMessage(chatInput);
    chatInput = '';
  }

  async function createRoom() {
    if (busy) return;
    busy = true;
    try {
      const displayName = settingsStore.ytAccountName || 'LastWave User';
      await ipcInvoke('jam_create_room', {
        displayName,
        collaborative: true,
        browserGuests: true,
      });
      await refreshSnapshot();
      featuresStore.showToast({
        title: 'Jam session started',
        subtitle: featuresStore.jamRoomCode ? `Room ${featuresStore.jamRoomCode}` : undefined,
        severity: 'success',
      });
    } finally {
      busy = false;
    }
  }

  async function joinRoom() {
    const code = joinCode.trim().toUpperCase();
    if (!code || busy) return;
    busy = true;
    try {
      const displayName = settingsStore.ytAccountName || 'LastWave User';
      await ipcInvoke('jam_join_room', { roomCode: code, displayName });
      await refreshSnapshot();
    } finally {
      busy = false;
    }
  }

  async function leaveRoom() {
    await ipcInvoke('jam_leave');
    featuresStore.jamConnected = false;
    featuresStore.jamRoomCode = '';
    featuresStore.jamParticipants = [];
    featuresStore.jamChat = [];
  }
</script>

<div class="space-y-6 px-8 py-8 pb-10">
  <CommandBar title="LastWave Jam" subtitle="Real-time synchronized listening rooms">
    {#snippet actions()}
      {#if featuresStore.jamConnected}
        <Buttons variant="danger" size="sm" onclick={() => void leaveRoom()}>Leave Room</Buttons>
      {:else}
        <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void createRoom()}>
          {busy ? 'Starting...' : 'Start Room'}
        </Buttons>
      {/if}
    {/snippet}
  </CommandBar>

  {#if !featuresStore.jamConnected}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
      <div class="rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-6">
        <h2 class="text-[16px] font-bold">Host a Jam</h2>
        <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">
          Share your live playback queue and sync timestamps with friends.
        </p>
        <Buttons variant="primary" class="mt-4" disabled={busy} onclick={() => void createRoom()}>Create Room</Buttons>
      </div>
      <div class="rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-6">
        <h2 class="text-[16px] font-bold">Join by Code</h2>
        <div class="mt-3 flex gap-2">
          <Input bind:value={joinCode} placeholder="Enter room code" />
          <Buttons variant="secondary" disabled={busy} onclick={() => void joinRoom()}>Join</Buttons>
        </div>
      </div>
    </div>
  {:else}
    <div class="grid grid-cols-1 gap-5 lg:grid-cols-3">
      <div class="space-y-4 lg:col-span-2">
        {#if playerStore.currentTrack}
          <div class="flex items-center gap-4 rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-4">
            <Artwork src={playerStore.currentTrack.artworkUrl} size={64} radius={12} />
            <div class="min-w-0 flex-1">
              <div class="text-[11px] font-bold tracking-[1px] text-[var(--lw-success)] uppercase">Live Sync · {featuresStore.jamRoomCode}</div>
              <div class="truncate text-[16px] font-bold">{playerStore.currentTrack.title}</div>
              <div class="truncate text-[12.5px] text-[var(--lw-text-secondary)]">{playerStore.currentTrack.artist}</div>
            </div>
          </div>
        {/if}

        <div class="flex h-[340px] flex-col rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-4">
          <div class="lw-scroll flex-1 space-y-2.5 pr-1">
            {#each featuresStore.jamChat as msg (msg.id)}
              <div class="rounded-[12px] bg-[var(--lw-surface-raised)] px-3 py-2">
                <div class="flex items-center justify-between text-[11px]">
                  <span class="font-semibold text-[var(--lw-text-primary)]">{msg.sender}</span>
                  <span class="text-[var(--lw-text-tertiary)] lw-tabular">{msg.time}</span>
                </div>
                <p class="mt-0.5 text-[13px] text-[var(--lw-text-secondary)]">{msg.text}</p>
              </div>
            {/each}
          </div>
          <form onsubmit={sendChat} class="mt-3 flex gap-2">
            <Input bind:value={chatInput} placeholder="Send a message to the room..." />
            <Buttons type="submit" variant="primary">Send</Buttons>
          </form>
        </div>
      </div>

      <div class="rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-4">
        <h3 class="text-[13.5px] font-bold">Listeners ({featuresStore.jamParticipants.length})</h3>
        <div class="mt-3 space-y-2">
          {#each featuresStore.jamParticipants as p (p.id)}
            <div class="flex items-center justify-between rounded-[10px] bg-[var(--lw-surface-raised)] px-3 py-2 text-[12.5px]">
              <span class="flex items-center gap-2"><WaveIcon name="artist" size={14} /> {p.name}</span>
              <span class="text-[11px] text-[var(--lw-text-tertiary)] uppercase">{p.role}</span>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>
