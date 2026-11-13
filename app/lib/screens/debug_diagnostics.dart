import 'package:material_ui/material_ui.dart';

import '../core/provider_diagnostics.dart';
import '../ui/console.dart';

class DebugDiagnostics extends StatelessWidget {
  final ProviderDiagnostics? snapshot;
  final ValueChanged<int> onPage;

  const DebugDiagnostics({
    super.key,
    required this.snapshot,
    required this.onPage,
  });

  @override
  Widget build(BuildContext context) {
    final data = snapshot;
    final palette = ConsolePalette.of(context);
    final style = TextStyle(
      fontFamily: 'monospace',
      fontSize: 11,
      height: 1.45,
      color: palette.text1,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Provider diagnostics${data == null ? '' : ' · ${data.driver}'}',
          style: style.copyWith(fontSize: 12, color: palette.cyan),
        ),
        if (data == null)
          Text(
            'Waiting for provider diagnostics. Use refresh to request again.',
            style: style,
          )
        else ...[
          Text(data.description, style: style),
          if (data.supported) ...[
            const SizedBox(height: 8),
            ConsoleActionGroup(
              children: [
                ConsoleButton(
                  label: 'Previous values',
                  onPressed: data.offset > 0
                      ? () => onPage((data.offset - 64).clamp(0, data.total))
                      : null,
                ),
                ConsoleButton(
                  label: 'Next values',
                  onPressed:
                      data.rows.isNotEmpty &&
                          data.offset + data.rows.length < data.total
                      ? () => onPage(data.offset + data.rows.length)
                      : null,
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              '${data.total == 0 ? 0 : data.offset + 1}–${data.offset + data.rows.length} of ${data.total} received entries',
              style: style,
            ),
            for (final row in data.rows)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 6),
                child: SelectableText(
                  '${row.key}${row.label == null ? '' : ' · ${row.label}'}\n${row.value}\n${row.status}',
                  style: style,
                ),
              ),
          ],
        ],
      ],
    );
  }
}
