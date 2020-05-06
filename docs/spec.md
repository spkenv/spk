

```yaml
pkg: <SPEC>
variants: <list.VARIANT>
depends: <list.PKG>
provides: <list.PKG>
```

```yaml
<PKG>:
   pkg: <SPEC>
   compat: <COMPAT_SPEC>
```

```yaml
<SPEC>: <NAME>/<VERSION>/<RELEASE>
<NAME>: [a-z][a-z0-9-]+
<VERSION>: \d+(\.\d)*
<RELEASE>: [a-z]+[0-9]+(\.[a-z]+[0-9]+)*
<COMPAT_SPEC>: x(\.x)*
```
