<!-- Primitive gallery: every generic primitive in every state, laid out
     compactly on the same 1280 canvas background. Stable ids #u-<name>. -->
<script lang="ts">
  import AppFrame from '../AppFrame.svelte';
  import Button from '../primitives/Button.svelte';
  import IconButton from '../primitives/IconButton.svelte';
  import Segmented from '../primitives/Segmented.svelte';
  import Toggle from '../primitives/Toggle.svelte';
  import Panel from '../primitives/Panel.svelte';
  import Pill from '../primitives/Pill.svelte';
  import Icon from '../primitives/Icon.svelte';

  const noop = () => {};
  let toggleVal = $state(true);
  let segVal = $state('12');
</script>

<AppFrame>
  <div
    style="flex:1;min-height:0;overflow-y:auto;padding:12px;background:var(--u4-page-bg)"
  >
    <div style="max-width:1100px;margin:0 auto;display:flex;flex-direction:column;gap:8px">
      <div style="font-size:17px;font-weight:600;color:var(--u4-text-1)">UI primitives gallery</div>

      <section id="u-buttons" aria-label="Buttons" style="display:flex;flex-wrap:wrap;gap:8px;align-items:center">
        <Button label="Default 40" onclick={noop} />
        <Button label="Default 44" height={44} onclick={noop} />
        <Button label="Compact" height={30} fontSize={13} padding="0 10px" onclick={noop} />
        <Button label="Primary" variant="primary" bold onclick={noop} />
        <Button label="MUTE on" variant="danger" tone="on" height={40} fontSize={15} onclick={noop} />
        <Button label="MUTE off" variant="danger" tone="off" height={40} fontSize={15} onclick={noop} />
        <Button label="Disabled" disabled onclick={noop} />
        <Button label="Selected" selected pressed={true} onclick={noop} />
      </section>

      <section id="u-iconbuttons" aria-label="Icon buttons" style="display:flex;gap:4px">
        <IconButton label="Effects" onclick={noop}><span style="font-size:16px;font-weight:600">FX</span></IconButton>
        <IconButton label="Routing" active onclick={noop}><Icon name="routing" size={22} /></IconButton>
        <IconButton label="Connection: online, select device" dot="#5fe060" onclick={noop}
          ><Icon name="plug" size={22} /></IconButton
        >
        <IconButton label="Settings" onclick={noop}><Icon name="gear" size={22} /></IconButton>
      </section>

      <section id="u-segmented" aria-label="Segmented" style="display:flex;flex-direction:column;gap:8px">
        <Segmented
          groupLabel="Strips per screen"
          options={[
            { value: '8', label: '8', selected: segVal === '8' },
            { value: '12', label: '12', selected: segVal === '12' },
            { value: '16', label: '16', selected: segVal === '16' },
          ]}
          onpick={(v) => (segVal = v)}
        />
        <Segmented
          groupLabel="Scene recall"
          options={[
            { value: 'confirm', label: 'Confirm', selected: true },
            { value: 'instant', label: 'Instant', selected: false },
          ]}
          onpick={noop}
        />
        <Segmented
          groupLabel="Effect type"
          options={[
            { value: 'hall', label: 'Hall', selected: true },
            { value: 'plate', label: 'Plate', selected: false },
            { value: 'delay', label: 'Delay', selected: false },
          ]}
          height={44}
          onpick={noop}
        />
      </section>

      <section id="u-toggle" aria-label="Toggle">
        <Toggle label="Fine control" value={toggleVal} onchange={(v) => (toggleVal = v)} />
      </section>

      <section id="u-panel" aria-label="Panels">
        <Panel title="Display">
          <div style="padding:12px 14px;font-size:15px;color:var(--u4-text-1)">Panel body</div>
        </Panel>
      </section>

      <section id="u-pills" aria-label="Pills" style="display:flex;gap:8px;align-items:center">
        <Pill tone="light">Pre</Pill>
        <Pill tone="dark">Post</Pill>
        <Pill tone="green">On</Pill>
        <Pill tone="amber">Bypassed</Pill>
      </section>

      <section id="u-icons" aria-label="Icons" style="display:flex;gap:12px;color:var(--u4-text-4);align-items:center">
        <Icon name="back" size={18} />
        <Icon name="lock" size={16} />
        <Icon name="routing" size={22} />
        <Icon name="plug" size={22} />
        <Icon name="gear" size={22} />
        <Icon name="chevronDown" size={16} stroke="#a7b1c0" />
        <Icon name="console" size={24} />
        <Icon name="io" size={24} />
        <Icon name="chevronLeft" size={20} />
        <Icon name="chevronRight" size={20} />
      </section>
    </div>
  </div>
</AppFrame>
