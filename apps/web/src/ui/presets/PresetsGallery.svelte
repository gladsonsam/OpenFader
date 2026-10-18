<!-- Presets gallery (`?gallery=presets`): isolated verification surface for
     the channel presets card. Fixture data only (no live link): an input
     channel with the full tone plane, plus a DCA with no preamp/EQ/dyn so
     the collapse + skip reporting renders deterministically. -->
<script lang="ts">
  import PresetsCard from '../screens/channel/PresetsCard.svelte';
</script>

<div
  style="box-sizing:border-box;display:flex;flex-direction:column;gap:16px;padding:16px;max-width:900px;margin:0 auto;background:#070b12;min-height:100vh"
>
  <h1 style="font-size:18px;color:#eef2f5;margin:0">Channel presets · gallery</h1>
  <div style="font-size:12px;color:#8792a4">
    Fixture mode — recalls edit the fixture model only. Full-tone strip first (in1), collapsed strip second (dca1).
  </div>
  <PresetsCard props={{ channelId: 'in1' }} ctx={{} as never} nodeId="presets-gallery-in1" />
  <PresetsCard props={{ channelId: 'dca1' }} ctx={{} as never} nodeId="presets-gallery-dca1" />
</div>
