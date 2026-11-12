import 'package:material_ui/material_ui.dart';

import '../palette.dart';
import 'remote_host.dart';

/// Browser entry connects only to the origin serving the real Flutter app.
class BrowserConnectionGate extends StatefulWidget {
  const BrowserConnectionGate({super.key, required this.connected});
  final Widget Function(RemoteHost host) connected;
  @override
  State<BrowserConnectionGate> createState() => _BrowserConnectionGateState();
}

class _BrowserConnectionGateState extends State<BrowserConnectionGate> {
  final _password = TextEditingController();
  RemoteServer? _server;
  RemoteHost? _host;
  bool _pending = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _inspect();
  }

  Future<void> _inspect() async {
    setState(() {
      _pending = true;
      _error = null;
    });
    try {
      final server = await RemoteHost.inspect('');
      if (!mounted) return;
      _server = server;
      if (!server.passwordRequired) {
        await _connect();
      } else {
        setState(() => _pending = false);
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _pending = false;
          _error = error.toString();
        });
      }
    }
  }

  Future<void> _connect() async {
    final server = _server;
    if (server == null) return;
    setState(() {
      _pending = true;
      _error = null;
    });
    try {
      final host = await RemoteHost.connect(server,
          password: server.passwordRequired ? _password.text : null);
      _password.clear();
      if (!mounted) {
        await host.disconnect();
        return;
      }
      setState(() => _host = host);
    } catch (error) {
      if (mounted) {
        setState(() {
          _pending = false;
          _error = error.toString();
        });
      }
    }
  }

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final host = _host;
    if (host != null) return widget.connected(host);
    final palette = ConsolePalette.of(context);
    return Scaffold(
        backgroundColor: palette.page,
        body: Center(
            child: SingleChildScrollView(
                child: SizedBox(
                    width: 400,
                    child: Padding(
                        padding: const EdgeInsets.all(24),
                        child: Card(
                            child: Padding(
                                padding: const EdgeInsets.all(24),
                                child: Column(
                                    mainAxisSize: MainAxisSize.min,
                                    crossAxisAlignment:
                                        CrossAxisAlignment.stretch,
                                    children: [
                                      const Text('OpenFader',
                                          style: TextStyle(
                                              fontSize: 28,
                                              fontWeight: FontWeight.w600)),
                                      const SizedBox(height: 12),
                                      Text(_server?.passwordRequired == true
                                          ? 'Enter the server password to connect.'
                                          : 'Connecting to the OpenFader server…'),
                                      if (_server?.passwordRequired ==
                                          true) ...[
                                        const SizedBox(height: 16),
                                        TextField(
                                            controller: _password,
                                            obscureText: true,
                                            autofocus: true,
                                            enabled: !_pending,
                                            autocorrect: false,
                                            enableSuggestions: false,
                                            textInputAction: TextInputAction.go,
                                            decoration: const InputDecoration(
                                                labelText: 'Server password'),
                                            onSubmitted: (_) {
                                              if (!_pending) _connect();
                                            }),
                                      ],
                                      if (_error != null) ...[
                                        const SizedBox(height: 12),
                                        Text(_error!,
                                            style:
                                                TextStyle(color: palette.mute)),
                                      ],
                                      const SizedBox(height: 16),
                                      if (_pending)
                                        const Center(
                                            child: CircularProgressIndicator())
                                      else
                                        FilledButton(
                                            onPressed:
                                                _server?.passwordRequired ==
                                                        true
                                                    ? _connect
                                                    : _inspect,
                                            child: Text(
                                                _server?.passwordRequired ==
                                                        true
                                                    ? 'Connect'
                                                    : 'Retry')),
                                    ]))))))));
  }
}
