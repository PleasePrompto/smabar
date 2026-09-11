---
title: Datenschutzhinweise der Desktop-App
description: Informationen nach Art. 13 DSGVO zu den Verbindungen, die die Desktop-Anwendung smabar aufbaut.
updated: 2026-09-11
---

## 1. Verantwortlicher

Verantwortlich für die hier beschriebene Verarbeitung ist:

Gérôme Dexheimer\
Wiener Straße 74\
48145 Münster\
Deutschland\
E-Mail: hello@smabar.com

Diese Hinweise betreffen die Desktop-Anwendung smabar. Für den Besuch von smabar.com gilt die Datenschutzerklärung der Website unter https://smabar.com/de/privacy/.

## 2. Welche Verbindungen die App aufbaut

smabar läuft auf Ihrem Rechner, benötigt kein Konto, zeigt keine Werbung und sendet keine Nutzungs- oder Telemetriedaten. Verbindungen entstehen nur in den folgenden Fällen. Der angefragte Server erhält dabei Ihre IP-Adresse, den Zeitpunkt und die technischen Angaben der Anfrage.

- **Update-Prüfung.** Direktinstallationen (DEB, RPM, Windows-Installer, macOS-DMG) rufen etwa eine Minute nach dem Start und danach alle sechs Stunden https://updates.smabar.com/latest.json ab. Die Anfrage enthält keine smabar-Version und keine Gerätekennung. Ein Update wird nur nach Ihrer Bestätigung von derselben Adresse geladen. updates.smabar.com wird über Cloudflare, Inc., 101 Townsend St., San Francisco, CA 94107, USA, ausgeliefert. Bei einer Installation aus dem Microsoft Store entfällt diese Prüfung; Updates liefert dann Microsoft nach eigenen Datenschutzbestimmungen.
- **Community-Katalog.** Etwa 90 Sekunden nach dem Start, danach alle sechs Stunden und vor jeder Installation lädt smabar den signierten Katalog von https://store.smabar.com. Die Anfrage enthält den User-Agent `smabar/<Version>` und die Kennung des zuletzt geladenen Katalogs (ETag), damit ein unveränderter Katalog nicht erneut übertragen wird. Beim Öffnen eines Eintrags lädt smabar dessen Beschreibung von derselben Adresse. store.smabar.com läuft auf einem von uns verwalteten Server; die Verbindung wird über Cloudflare geleitet.
- **Mitgelieferte Plugins mit Datenabruf.** Drei der mitgelieferten Plugins holen Daten von Drittanbietern, solange sie aktiv sind, und zwar auch ohne weitere Einstellung: Das Wetter-Plugin fragt alle 15 Minuten die Vorhersage für die eingestellten Orte (voreingestellt: Berlin) bei Open-Meteo (api.open-meteo.com) ab und übermittelt dabei die Koordinaten des Ortes; wenn Sie einen Ort hinzufügen, wird der eingegebene Ortsname an geocoding-api.open-meteo.com gesendet. Das Uhr-Plugin nutzt dieselbe Ortssuche, wenn Sie eine Weltzeituhr hinzufügen. Das Krypto-Plugin fragt alle fünf Minuten die Kurse der eingestellten Coins (voreingestellt: Bitcoin und Ethereum in Euro) bei CoinGecko (api.coingecko.com) ab. Diese Dienste erhalten dabei Ihre IP-Adresse; es gelten ihre Datenschutzbestimmungen unter https://open-meteo.com und https://www.coingecko.com. Deaktivieren Sie ein Plugin unter Einstellungen › Plugins, wird sein Prozess beendet und es findet kein Abruf mehr statt; das bloße Ausblenden des Plugins genügt dafür nicht. Die Plugins Systeminfo, Medien und Aufgaben arbeiten ohne Netzverbindung.
- **Plugin- und Theme-Downloads.** Bei einer von Ihnen bestätigten Installation lädt smabar das Plugin-Archiv oder die Theme-Datei direkt von GitHub (github.com, codeload.github.com, raw.githubusercontent.com) mit dem User-Agent `smabar/<Version>`. Enthält die Beschreibung eines Eintrags Bilder, lädt smabar diese ebenfalls von GitHub. Dabei erhält GitHub, Inc., 88 Colin P. Kelly Jr. Street, San Francisco, CA 94107, USA, Ihre IP-Adresse; es gilt die Datenschutzerklärung von GitHub.
- **Symbole angepinnter Webseiten.** Wenn Sie eine Webseite an die Leiste pinnen, lädt smabar deren Symbol direkt von dieser Webseite (`/apple-touch-icon.png`, `/favicon.ico`) und speichert es unter `~/.smabar/cache/icons/`. Ein Symboldienst eines Dritten ist nicht beteiligt.
- **Google Fonts.** Nur wenn Sie oder ein von Ihnen verbundener Agent in den Einstellungen oder in einem Theme eine Google-Schrift wählen, lädt smabar die Schriftdateien einmalig von fonts.googleapis.com und fonts.gstatic.com (Google Ireland Limited, Gordon House, Barrow Street, Dublin 4, Irland) und speichert sie unter `~/.smabar/cache/fonts/google/`. Die mitgelieferten Themes verwenden Systemschriften und lösen keinen Abruf aus.
- **Community-Plugins und Agenten.** Plugins aus dem Community Store sind eigene Programme und können eigene Verbindungen aufbauen, etwa zu einem Paketdienst oder einem Gerät in Ihrem Netz. Welche Daten ein solches Plugin überträgt, bestimmt die Person, die es veröffentlicht hat; diese Hinweise erfassen das nicht. Der MCP-Server von smabar nimmt nur Verbindungen von Ihrem eigenen Rechner an (127.0.0.1). Was ein von Ihnen verbundener Agent mit gelesenen Daten tut, richtet sich nach dessen Anbieter.

## 3. Rechtsgrundlage, Speicherdauer und Widerspruch

Rechtsgrundlage für diese Verbindungen ist Art. 6 Abs. 1 lit. f DSGVO. Unser berechtigtes Interesse liegt darin, Updates und den signierten Katalog bereitzustellen und die von Ihnen gewählten Inhalte und Plugins mit Daten zu versorgen. Auf unseren Servern erscheint Ihre IP-Adresse nur in technischen Logs, die nach spätestens 14 Tagen gelöscht werden, sofern kein Sicherheitsvorfall eine längere Aufbewahrung erfordert. Cloudflare, GitHub und Google können Daten auch in den USA verarbeiten; alle drei sind nach dem EU-U.S. Data Privacy Framework zertifiziert. Für Open-Meteo und CoinGecko gelten deren eigene Angaben zum Verarbeitungsort.

Sie können den Verbindungen widersprechen, indem Sie die jeweilige Funktion nicht nutzen: Wetter- und Krypto-Plugin deaktivieren, keine Weltzeituhr hinzufügen, keine Store-Installation, keine Google-Schrift, keine angepinnte Webseite. Die Update-Prüfung lässt sich in Direktinstallationen nicht einzeln abschalten; wer sie nicht wünscht, nutzt die Version aus dem Microsoft Store oder deinstalliert smabar.

## 4. Lokale Daten

smabar speichert Einstellungen, Plugins, Plugin-Daten, Caches und Logs unter `~/.smabar` (Linux und macOS; Windows: `%USERPROFILE%\.smabar`). Diese Daten verlassen Ihren Rechner nicht; Logs werden nicht an uns übertragen. Sie können die Daten jederzeit einsehen und löschen. Beim Entfernen eines Plugins löscht smabar dessen Code, Daten und Logs; das Deinstallieren von smabar lässt `~/.smabar` unberührt.

## 5. Ihre Rechte

Sie haben nach der DSGVO das Recht auf Auskunft (Art. 15), Berichtigung (Art. 16), Löschung (Art. 17), Einschränkung der Verarbeitung (Art. 18), Datenübertragbarkeit (Art. 20) und Widerspruch (Art. 21). Sie können sich bei einer Datenschutzaufsichtsbehörde beschweren, etwa bei der Landesbeauftragten für Datenschutz und Informationsfreiheit Nordrhein-Westfalen, Kavalleriestraße 2-4, 40213 Düsseldorf. Anfragen richten Sie an hello@smabar.com. Da smabar keine Konten führt und IP-Adressen nur kurz in Logs vorkommen, können wir eine Anfrage in der Regel keiner Person zuordnen (Art. 11 DSGVO).

Wir passen diese Hinweise an, wenn sich die App oder die eingesetzten Dienste ändern. Das Datum am Anfang nennt den aktuellen Stand; eine Zustimmung ist dafür nicht erforderlich.
