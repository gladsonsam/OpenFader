// Connection screen: scan, probe, connect, and disconnect desks.
// Screens stay constructor-injected so Area A can implement the store.
import 'package:material_ui/material_ui.dart';

import '../theme.dart';
import '../ui/console.dart';

// Automatic asks every registered hardware driver to identify the host.
enum MixerType {
  automatic('Automatic', 'auto'),
  xair('Behringer X-Air', 'xair'),
  x32('Behringer X32', 'x32'),
  dl32s('Mackie DL32S', 'dl32s'),
  wing('Behringer WING', 'wing'),
  server('OpenFader server', 'openfader-server');

  const MixerType(this.label, this.driver);
  final String label;
  final String driver;

  static MixerType fromDriver(String driver) => values
      .firstWhere((type) => type.driver == driver, orElse: () => automatic);
}

class MixerTypeField extends StatelessWidget {
  final MixerType value;
  final ValueChanged<MixerType>? onChanged;
  final bool includeServer;
  const MixerTypeField(
      {super.key,
      required this.value,
      this.onChanged,
      this.includeServer = false});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return DropdownButtonFormField<MixerType>(
      initialValue: value,
      isExpanded: true,
      dropdownColor: palette.chip,
      style: TextStyle(fontSize: 17, color: palette.text),
      decoration: InputDecoration(
        labelText: 'Mixer type',
        labelStyle: TextStyle(color: palette.mutedText),
        filled: true,
        fillColor: palette.well,
        enabledBorder: OutlineInputBorder(
          borderSide: BorderSide(color: palette.lineCtl),
          borderRadius: BorderRadius.circular(5),
        ),
        focusedBorder: OutlineInputBorder(
          borderSide: BorderSide(color: palette.lime),
          borderRadius: BorderRadius.circular(5),
        ),
        contentPadding:
            const EdgeInsets.symmetric(horizontal: 14, vertical: 14),
      ),
      items: [
        for (final type in MixerType.values
            .where((type) => includeServer || type != MixerType.server))
          DropdownMenuItem(
              value: type,
              child: Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: Text(type.label,
                      maxLines: 1, overflow: TextOverflow.ellipsis))),
      ],
      onChanged: onChanged == null ? null : (type) => onChanged!(type!),
    );
  }
}

class ConnDevice {
  final String id;
  final String name;
  final String sub; // model · ip · fw
  final String state;
  final bool connected;
  final MixerType mixerType;

  const ConnDevice({
    required this.id,
    required this.name,
    required this.sub,
    this.state = '',
    this.connected = false,
    this.mixerType = MixerType.automatic,
  });
}

// Implemented by the shell over the live engine.
abstract class ConnectionStore extends ChangeNotifier {
  String get title;
  String get sub;
  bool get isConnected;
  bool get isOffline;
  bool get scanning;
  String get lastScan;
  String? get scanError;
  String? get errorAt;
  String? get lastProbe;
  int get probeAttempt;
  bool get probing;
  List<ConnDevice> get devices;
  String? get emptyText;

  // Lets the start screen probe a row directly instead of parsing the display string.
  String hostOf(String id);
  String canonicalHost(String host);

  String get connectedHost;
  String get linkInfo;
  String get deskInfo;
  int get clientCount;
  List<String> get writers;

  void scan();
  void refreshKnown(List<String> hosts);
  String? checkStatusOf(String host);
  MixerType get lastMixerType;
  String get connectionProgress;
  void probe(String addr, {MixerType mixerType = MixerType.automatic});
  Future<void> retryProbe();
  void connectDevice(String id);
  void connectSim();
  void disconnect();
  void requestInfo();
  void clearError();
  Future<bool> ensurePermissions();
}

class U4ConnectionPage extends StatefulWidget {
  final ConnectionStore store;
  final VoidCallback? onOpenDebug;

  const U4ConnectionPage({super.key, required this.store, this.onOpenDebug});

  @override
  State<U4ConnectionPage> createState() => _U4ConnectionPageState();
}

class _U4ConnectionPageState extends State<U4ConnectionPage> {
  final _ipCtl = TextEditingController();
  MixerType _mixerType = MixerType.automatic;

  @override
  void dispose() {
    _ipCtl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            final palette = ConsolePalette.of(context);
            return SingleChildScrollView(
              padding: AppDesign.pagePadding,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _StatusCard(store: store),
                  const SizedBox(height: AppDesign.spaceLg),
                  Text('CONNECT A MIXER',
                      style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: palette.text4)),
                  const SizedBox(height: AppDesign.spaceXs),
                  Text(
                    store.isOffline
                        ? 'Find a mixer on this network or connect directly by IP.'
                        : 'Find a mixer on this network, or continue without hardware.',
                    style: TextStyle(fontSize: 12, color: palette.text7),
                  ),
                  const SizedBox(height: AppDesign.spaceMd),
                  Row(
                    children: [
                      Expanded(
                        child: _Btn(
                          label: store.scanning ? 'Searching…' : 'Find mixers',
                          enabled: !store.scanning,
                          primary: true,
                          onTap: store.scan,
                        ),
                      ),
                      if (!store.isConnected) ...[
                        const SizedBox(width: AppDesign.spaceSm),
                        Expanded(
                          child: _Btn(
                            label: 'Work offline',
                            onTap: store.connectSim,
                          ),
                        ),
                      ],
                      if (store.isConnected && !store.isOffline) ...[
                        const SizedBox(width: AppDesign.spaceSm),
                        Expanded(
                          child: _Btn(
                            label: 'Disconnect',
                            danger: true,
                            onTap: store.disconnect,
                          ),
                        ),
                      ],
                    ],
                  ),
                  if (store.lastScan != 'never')
                    Padding(
                      padding: const EdgeInsets.only(top: AppDesign.spaceXs),
                      child: Text('Last search: ${store.lastScan}',
                          style: TextStyle(fontSize: 11, color: palette.text7)),
                    ),
                  if (store.scanning)
                    Padding(
                      padding: EdgeInsets.only(top: 4),
                      child: Text('Waiting for mixers to reply…',
                          style: TextStyle(
                              fontSize: 12,
                              color: ConsolePalette.of(context).text4)),
                    ),
                  if (store.scanError != null) _ErrorBanner(store: store),
                  const SizedBox(height: AppDesign.spaceLg),
                  Text('AVAILABLE MIXERS',
                      style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: ConsolePalette.of(context).text4)),
                  const SizedBox(height: 6),
                  if (store.devices.isEmpty)
                    Text(
                      store.emptyText ??
                          'Scan this network to find available mixers.',
                      style: TextStyle(
                          fontSize: 12,
                          color: ConsolePalette.of(context).text7),
                    )
                  else
                    for (final d in store.devices)
                      _DeviceRow(
                        device: d,
                        onConnect: () => store.connectDevice(d.id),
                      ),
                  const SizedBox(height: AppDesign.spaceLg),
                  Text('CONNECT MANUALLY',
                      style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: ConsolePalette.of(context).text4)),
                  const SizedBox(height: 6),
                  MixerTypeField(
                    value: _mixerType,
                    onChanged: store.probing
                        ? null
                        : (type) => setState(() => _mixerType = type),
                  ),
                  const SizedBox(height: AppDesign.spaceSm),
                  Row(
                    children: [
                      Expanded(
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          decoration: BoxDecoration(
                            color: ConsolePalette.of(context).well,
                            border: Border.all(
                                color: ConsolePalette.of(context).railBorder),
                            borderRadius: U4.rBtn,
                          ),
                          child: TextField(
                            controller: _ipCtl,
                            keyboardType: TextInputType.text,
                            style: TextStyle(
                                fontSize: 13,
                                color: ConsolePalette.of(context).text1),
                            decoration: InputDecoration(
                              isDense: true,
                              border: InputBorder.none,
                              enabledBorder: InputBorder.none,
                              disabledBorder: InputBorder.none,
                              focusedBorder: UnderlineInputBorder(
                                  borderSide: BorderSide(
                                      color: ConsolePalette.of(context).cyan)),
                              filled: false,
                              contentPadding:
                                  EdgeInsets.symmetric(vertical: 12),
                              hintText: '192.168.1.120',
                              hintStyle: TextStyle(
                                  color: ConsolePalette.of(context).text10),
                            ),
                            onSubmitted: (v) {
                              final a = v.trim();
                              if (a.isNotEmpty) {
                                FocusScope.of(context).unfocus();
                                store.probe(a, mixerType: _mixerType);
                              }
                            },
                          ),
                        ),
                      ),
                      const SizedBox(width: 8),
                      _Btn(
                        label: store.probing ? 'Connecting…' : 'Connect',
                        enabled: !store.probing,
                        onTap: () {
                          final a = _ipCtl.text.trim();
                          if (a.isNotEmpty) {
                            FocusScope.of(context).unfocus();
                            store.probe(a, mixerType: _mixerType);
                          }
                        },
                      ),
                    ],
                  ),
                  if (store.probing)
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: Text(store.connectionProgress,
                          style: TextStyle(
                              fontSize: 12,
                              color: ConsolePalette.of(context).text4)),
                    ),
                  const SizedBox(height: AppDesign.spaceLg),
                  ExpansionTile(
                    title: const Text('Technical details'),
                    subtitle: const Text('Link, simulator, and client status'),
                    tilePadding: EdgeInsets.zero,
                    childrenPadding: EdgeInsets.zero,
                    expansionAnimationStyle: AnimationStyle.noAnimation,
                    children: [
                      _InfoCard(store: store, onOpenDebug: widget.onOpenDebug)
                    ],
                  ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  final ConnectionStore store;
  const _ErrorBanner({required this.store});

  @override
  Widget build(BuildContext context) {
    final err = store.scanError ?? '';
    final canRetry = (store.lastProbe ?? '').isNotEmpty && !store.probing;
    return Container(
      margin: const EdgeInsets.only(top: 8),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).errorContainer,
        border: Border.all(color: ConsolePalette.of(context).mute),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('DESK LINK FAILED',
              style: TextStyle(
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  color: ConsolePalette.of(context).onErrorContainer)),
          const SizedBox(height: AppDesign.spaceXs),
          Text(err,
              style: TextStyle(
                  fontSize: 12,
                  color: ConsolePalette.of(context).onErrorContainer)),
          const SizedBox(height: AppDesign.spaceSm),
          ConsoleActionGroup(children: [
            if (canRetry)
              _Btn(
                  label:
                      'Retry ${store.lastMixerType.label} · ${store.lastProbe}',
                  primary: true,
                  onTap: store.retryProbe),
            _Btn(label: 'Rescan', onTap: store.scan),
            _Btn(label: 'Dismiss', onTap: store.clearError),
          ]),
        ],
      ),
    );
  }
}

class _StatusCard extends StatelessWidget {
  final ConnectionStore store;
  const _StatusCard({required this.store});

  @override
  Widget build(BuildContext context) {
    final dot = store.isConnected
        ? ConsolePalette.of(context).meterGreen
        : ConsolePalette.of(context).mute;
    return Container(
      padding: const EdgeInsets.all(AppDesign.spaceMd),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(
            color: store.isConnected
                ? ConsolePalette.of(context).cyan
                : ConsolePalette.of(context).mute),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Row(
        children: [
          Container(
            width: 10,
            height: 10,
            decoration: BoxDecoration(
              color: dot,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(store.title,
                    style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w700,
                        color: ConsolePalette.of(context).text1)),
                Text(
                    store.isOffline
                        ? 'No hardware mixer connected. Mixer controls remain available.'
                        : store.sub,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontSize: 12, color: ConsolePalette.of(context).text7)),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _DeviceRow extends StatelessWidget {
  final ConnDevice device;
  final VoidCallback onConnect;

  const _DeviceRow({required this.device, required this.onConnect});

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(
            color: device.connected
                ? ConsolePalette.of(context).cyan
                : ConsolePalette.of(context).tileBorder),
        borderRadius: U4.rBtn,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(device.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                        color: ConsolePalette.of(context).text1)),
                Text(device.sub,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontSize: 11, color: ConsolePalette.of(context).text7)),
              ],
            ),
          ),
          Text(
            device.connected
                ? 'Connected'
                : device.state.isEmpty
                    ? 'Available'
                    : device.state,
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w700,
              color: device.connected
                  ? ConsolePalette.of(context).meterGreen
                  : ConsolePalette.of(context).text4,
            ),
          ),
          const SizedBox(width: AppDesign.spaceSm),
          if (!device.connected)
            GestureDetector(
              onTap: onConnect,
              child: Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(
                  color: ConsolePalette.of(context).cyan,
                  borderRadius: U4.rBtn,
                ),
                child: Text('Connect',
                    textAlign: TextAlign.center,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w700,
                        color: ConsolePalette.of(context).onAccent)),
              ),
            ),
        ],
      ),
    );
  }
}

class _InfoCard extends StatelessWidget {
  final ConnectionStore store;
  final VoidCallback? onOpenDebug;
  const _InfoCard({required this.store, this.onOpenDebug});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(color: ConsolePalette.of(context).tileBorder),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _line(context, 'link', store.linkInfo),
          _line(context, 'desk', store.deskInfo),
          _line(context, 'clients',
              '${store.clientCount}${store.writers.isEmpty ? '' : ' · ${store.writers.join(',')}'}'),
          const SizedBox(height: 6),
          GestureDetector(
            onTap: store.requestInfo,
            child: Text('REFRESH DESK INFO',
                style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: ConsolePalette.of(context).cyan)),
          ),
          if (onOpenDebug != null) ...[
            const SizedBox(height: AppDesign.spaceXs),
            GestureDetector(
              onTap: onOpenDebug,
              child: Text('OPEN DEBUG LOG',
                  style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: ConsolePalette.of(context).cyan)),
            ),
          ],
        ],
      ),
    );
  }

  Widget _line(BuildContext context, String k, String v) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 56,
            child: Text(k,
                style: TextStyle(
                    fontSize: 11, color: ConsolePalette.of(context).text7)),
          ),
          Expanded(
            child: Text(v.isEmpty ? '-' : v,
                style: TextStyle(
                    fontSize: 11, color: ConsolePalette.of(context).text4)),
          ),
        ],
      ),
    );
  }
}

class _Btn extends StatelessWidget {
  final String label;
  final VoidCallback onTap;
  final bool enabled;
  final bool primary;
  final bool danger;

  const _Btn({
    required this.label,
    required this.onTap,
    this.enabled = true,
    this.primary = false,
    this.danger = false,
  });

  @override
  Widget build(BuildContext context) {
    return ConsoleButton(
        label: label,
        primary: primary,
        danger: danger,
        onPressed: enabled ? onTap : null);
  }
}
