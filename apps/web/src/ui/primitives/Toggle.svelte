<!-- Off/On pair used in Settings rows (Fine, Sof, Mute Enable, Screen lock,
     Meter peak hold). Same colours as Segmented: selected light #e8edf5 /
     dark text, unselected #1b222d / #c9d1dc, borders #e8edf5 / #343d4b,
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
  darkBg="#1b222d"
  darkBorder="#343d4b"
  onpick={(v) => onchange(v === 'on')}
/>
