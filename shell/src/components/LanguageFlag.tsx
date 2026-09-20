import { Globe2 } from "lucide-react";

/** Decorative flags for bundled languages; custom locales get a neutral globe. */
export function LanguageFlag({ code }: { code: string }) {
  const language = code.toLowerCase().split(/[-_]/)[0];
  if (language !== "de" && language !== "en")
    return <Globe2 className="language-flag" aria-hidden="true" />;
  return (
    <svg className="language-flag" viewBox="0 0 60 40" aria-hidden="true">
      {language === "de" ? (
        <>
          <path fill="#242424" d="M0 0h60v14H0z" />
          <path fill="#d9383c" d="M0 14h60v13H0z" />
          <path fill="#f5c84c" d="M0 27h60v13H0z" />
        </>
      ) : (
        <>
          <path fill="#244678" d="M0 0h60v40H0z" />
          <path stroke="#f8f6ef" strokeWidth="10" d="m0 0 60 40M60 0 0 40" />
          <path stroke="#d8404b" strokeWidth="4" d="m0 0 60 40M60 0 0 40" />
          <path stroke="#f8f6ef" strokeWidth="14" d="M30 0v40M0 20h60" />
          <path stroke="#d8404b" strokeWidth="8" d="M30 0v40M0 20h60" />
        </>
      )}
    </svg>
  );
}
