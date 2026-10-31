# Fuentes y licencias

Esta skill adapta criterios de pruebas a Kotlin/JUnit, corrutinas y los contratos de
EmuSync. Las fuentes se investigaron el 2026-10-30 y se fijan a revisiones concretas
para que cambios posteriores de los repositorios no modifiquen la política local.

| Fuente | Revisión | Material usado |
| --- | --- | --- |
| [Matt Pocock / skills](https://github.com/mattpocock/skills/tree/a496f5513d4a4cf889bc48e189e579f4b0bdf6aa/skills/engineering/tdd) | `a496f5513d4a4cf889bc48e189e579f4b0bdf6aa` | `SKILL.md`, `tests.md` y `mocking.md`: comportamiento observable, expectativas independientes y límites de prueba. |
| [Jesse Vincent / Superpowers](https://github.com/obra/superpowers/blob/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/test-driven-development/writing-good-tests.md) | `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b` | `writing-good-tests.md`: fallos de producción detectables, tautologías y uso de dobles. |

La adaptación permite pruebas de efectos persistidos y de número/orden de llamadas
cuando representan contratos reales, y combina pruebas unitarias y de integración
según el riesgo. No impone TDD universal, eliminación de código escrito antes de las
pruebas, consulta por cada prueba ni dependencias de otras skills. No se instalaron
los paquetes comunitarios globalmente.

Los siguientes avisos MIT se conservan para el material adaptado. No cambian la
licencia del resto del proyecto.

## Matt Pocock

[Licencia de origen](https://github.com/mattpocock/skills/blob/a496f5513d4a4cf889bc48e189e579f4b0bdf6aa/LICENSE)

```text
MIT License

Copyright (c) 2026 Matt Pocock

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Jesse Vincent / Superpowers

[Licencia de origen](https://github.com/obra/superpowers/blob/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/LICENSE)

```text
MIT License

Copyright (c) 2025 Jesse Vincent

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
