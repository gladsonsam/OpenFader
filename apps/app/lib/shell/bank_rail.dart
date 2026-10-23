// H1 BankRail: the bank/layer rail. One continuous surface, not floating
// cards: square segments with hairline dividers, the active bank flooded
// with its colour edge-to-edge (console layer buttons).
//
// `orientation.side` is the desktop rail (w92 column); `orientation.bottom`
// is the phone bar (h56 row). Colours come from the bank descriptors
// (U4.banks); unselected tiles use the tint map, like the design.
library;

import 'package:flutter/material.dart';
import '../palette.dart';

/// Unselected bank-rail label tints (shellStyle.ts BANK_TINT).
Map<String, Color> _bankTint(ConsolePalette palette) => {
      'ch1': palette.meterGreen,
      'ch17': palette.meterGreen,
      'ch33': palette.meterGreen,
      'fx': palette.meterGreen,
      'mix': const Color(0xFF5FE3F0),
      'grp': const Color(0xFF8FB4FF),
      'mtx': const Color(0xFFEA8CF2),
      'dca': const Color(0xFFFF8A80),
    };

enum BankRailOrientation { side, bottom }

class BankRailBank {
  final String key;
  final String label;
  final Color color;
  final Color text;

  const BankRailBank(
      {required this.key,
      required this.label,
      required this.color,
      required this.text});
}

class U4BankRail extends StatelessWidget {
  final List<BankRailBank> banks;
  final String activeKey;
  final ValueChanged<String> onSelect;
  final BankRailOrientation orientation;

  const U4BankRail({
    super.key,
    required this.banks,
    required this.activeKey,
    required this.onSelect,
    this.orientation = BankRailOrientation.side,
  });

  @override
  Widget build(BuildContext context) {
    if (orientation == BankRailOrientation.bottom) {
      return Container(
        height: 56,
        decoration: BoxDecoration(
          color: ConsolePalette.of(context).panel,
          border: Border(
              top: BorderSide(color: ConsolePalette.of(context).linePanel)),
        ),
        child: Row(
          children: [
            for (var i = 0; i < banks.length; i++)
              Expanded(
                child: _Segment(
                  bank: banks[i],
                  active: banks[i].key == activeKey,
                  leadingDivider: i != 0,
                  vertical: false,
                  onSelect: onSelect,
                ),
              ),
          ],
        ),
      );
    }
    return Container(
      width: 92,
      color: ConsolePalette.of(context).panel,
      child: Column(
        children: [
          for (var i = 0; i < banks.length; i++)
            Expanded(
              child: _Segment(
                bank: banks[i],
                active: banks[i].key == activeKey,
                leadingDivider: i != 0,
                vertical: true,
                onSelect: onSelect,
              ),
            ),
        ],
      ),
    );
  }
}

class _Segment extends StatelessWidget {
  final BankRailBank bank;
  final bool active;
  final bool leadingDivider;
  final bool vertical;
  final ValueChanged<String> onSelect;

  const _Segment({
    required this.bank,
    required this.active,
    required this.leadingDivider,
    required this.vertical,
    required this.onSelect,
  });

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final bg = active ? bank.color : palette.bankOff;
    final tint = _bankTint(palette)[bank.key] ?? palette.text1;
    final fg = active
        ? bank.text
        : palette.isLight
            ? Color.lerp(tint, palette.text1, .55)!
            : tint;
    return GestureDetector(
      onTap: () => onSelect(bank.key),
      child: Container(
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(horizontal: 4),
        decoration: BoxDecoration(
          color: bg,
          border: vertical
              ? Border(
                  top: leadingDivider
                      ? BorderSide(color: ConsolePalette.of(context).linePanel)
                      : BorderSide.none,
                )
              : Border(
                  left: leadingDivider
                      ? BorderSide(color: ConsolePalette.of(context).linePanel)
                      : BorderSide.none,
                ),
        ),
        child: Text(
          bank.label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          textAlign: TextAlign.center,
          style: TextStyle(
            fontSize: vertical ? 15 : 12,
            fontWeight: active ? FontWeight.w600 : FontWeight.w500,
            color: fg,
          ),
        ),
      ),
    );
  }
}
