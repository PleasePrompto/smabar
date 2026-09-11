import { t } from "../../i18n/t";
import { useSmabar } from "../../store/bar";
import { Choice, ChoiceGrid } from "./controls";
import { setConfig } from "./persist";

/**
 * The app language as a segmented choice. Persisted through `update_config`;
 * the store follows the config event, and the locale change remounts the
 * panel. Shared by the System tab and the legal notice, which is shown
 * before any other setting is reachable.
 */
export function LanguageChoice({
  languages,
}: {
  languages: readonly string[];
}) {
  const language = useSmabar((state) => state.language);
  return (
    <ChoiceGrid label={t("settings.system.language")}>
      {languages.map((code) => (
        <Choice
          key={code}
          label={languageName(code)}
          active={code === language}
          onClick={() => {
            setConfig("language", code);
          }}
        />
      ))}
    </ChoiceGrid>
  );
}

/**
 * A language in its own language ("Deutsch", not "German"), which is what a
 * picker needs: someone looking for their language cannot read the current
 * one. Unknown codes fall back to the code itself — drop-in locales from
 * `~/.smabar/locales/` can be anything.
 */
function languageName(code: string): string {
  const names = new Intl.DisplayNames([code], {
    type: "language",
    fallback: "code",
  });
  const name = names.of(code) ?? code;
  return name.charAt(0).toUpperCase() + name.slice(1);
}
