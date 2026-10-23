// Startup header uses the shared console surfaces, typography and controls.
import 'package:flutter/material.dart';

import '../theme.dart';

class U4Header extends StatelessWidget {
  final String title;
  final bool connected;
  final VoidCallback? onBack;

  const U4Header({
    super.key,
    required this.title,
    required this.connected,
    this.onBack,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      height: U4.headerH,
      padding: const EdgeInsets.fromLTRB(12, 6, 8, 6),
      decoration: const BoxDecoration(
        color: U4.headerBg,
        border: Border(bottom: BorderSide(color: U4.lineHeader)),
      ),
      child: Row(
        children: [
          if (onBack != null)
            GestureDetector(
              onTap: onBack,
              child: Container(
                height: 44,
                padding: const EdgeInsets.fromLTRB(10, 0, 12, 0),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: U4.bankOff,
                  border: Border.all(color: U4.lineCtl),
                  borderRadius: U4.rBtn,
                ),
                child: const Text('‹ Back',
                    textAlign: TextAlign.center,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: U4.text1, fontSize: 14)),
              ),
            ),
          Expanded(
            child: Center(
              child: Text(
                title,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                    color: U4.text1, fontSize: 17, fontWeight: FontWeight.w600),
              ),
            ),
          ),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              const _Tile(label: 'FX'),
              const SizedBox(width: 4),
              const _Tile(icon: Icons.account_tree_outlined),
              const SizedBox(width: 4),
              _Tile(
                icon: Icons.power,
                dot: connected ? U4.meterGreen : U4.meterAmber,
              ),
              const SizedBox(width: 4),
              const _Tile(icon: Icons.settings_outlined),
            ],
          ),
        ],
      ),
    );
  }
}

class _Tile extends StatelessWidget {
  final String? label;
  final IconData? icon;
  final Color? dot;

  const _Tile({this.label, this.icon, this.dot});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 44,
      height: 44,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: U4.tileBg,
        border: Border.all(color: U4.divider),
        borderRadius: U4.rBtn,
      ),
      child: label != null
          ? const Text('FX',
              textAlign: TextAlign.center,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  color: U4.text4, fontSize: 16, fontWeight: FontWeight.w600))
          : Icon(icon, size: 22, color: dot ?? const Color(0xFFDFE6EF)),
    );
  }
}
