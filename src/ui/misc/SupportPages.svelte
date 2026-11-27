<script lang="ts">
  import CommandBar from '$src/ui/components/CommandBar.svelte';
</script>

<div class="mx-auto max-w-[820px] space-y-4 px-6 py-4">
  <CommandBar title="Help & Support" subtitle="Keyboard shortcuts and community links" />
  <div class="rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-5 text-[13px] text-[var(--lw-text-secondary)]">
    <ul class="space-y-2">
      <li><strong class="text-[var(--lw-text-primary)]">Space</strong> — Play / Pause</li>
      <li><strong class="text-[var(--lw-text-primary)]">Ctrl + K</strong> — Command Palette</li>
      <li><strong class="text-[var(--lw-text-primary)]">Ctrl + F</strong> — Jump to Search</li>
      <li><strong class="text-[var(--lw-text-primary)]">Q / L</strong> — Toggle Queue / Lyrics panel</li>
      <li><strong class="text-[var(--lw-text-primary)]">M</strong> — Mute / Unmute</li>
    </ul>
  </div>
</div>
