The FX table contains model-specific protocol facts extracted from
[dannyfiresnake/libwing propmap.jsonl](https://github.com/dannyfiresnake/libwing/blob/74c2ff649aabcc62a788fcf034c41d7d82e40b3c/propmap.jsonl).
Upstream revision: `74c2ff649aabcc62a788fcf034c41d7d82e40b3c`.
The upstream README identifies a WING Compact running firmware 3.0.5 as its source.
The upstream repository LICENSE (Apache-2.0) is retained as LICENSE.libwing.
OpenFader code remains GPL-3.0-or-later.

Only FX slot 1's model descriptions and slot 9's model availability were extracted;
other slots share these definitions. Synthetic model segments in libwing's
fullname values are not sent in OSC paths (manual p.24).
All controls are documentation/schema-based, not hardware-verified in OpenFader.
Do not run wingschema on a working mixer: its enumeration changes loaded models.

Regenerate from a local checkout of the pinned upstream revision with:

```sh
python3 crates/proto-wing/tools/import_schema.py /path/to/libwing/propmap.jsonl
```

The importer retains descending ranges, emits float32-preserving literals,
and omits one-step constant properties rather than presenting fake controls.
