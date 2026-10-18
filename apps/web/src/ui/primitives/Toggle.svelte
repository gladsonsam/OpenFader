<!-- Off/On pair used in Settings rows (Fine, Sof, Mute Enable, Screen lock,
     Meter peak hold). Same colours as Segmented: selected light var(--u4-text-1) /
     dark text, unselected var(--u4-seg-opt-bg) / var(--u4-text-4), borders var(--u4-text-1) / var(--u4-seg-border),
     h40 min-w 52 radius 3. -->
<script lang="ts">
  import Segmented from './Segmented.svelte';

  let {
    label,
    value,
    onchange,
  }: {
    label: string;
    value: boolean;
    onchange: (value: boolean) => void;
  } = $props();
</script>

<Segmented
  groupLabel={label}
  options={[
    { value: 'off', label: 'Off', selected: !value },
    { value: 'on', label: 'On', selected: value },
  ]}
  height={40}
  minWidth={52}
  radius={3}
  darkBg="var(--u4-seg-opt-bg)"
  darkBorder="var(--u4-seg-border)"
  onpick={(v) => onchange(v === 'on')}
/>
