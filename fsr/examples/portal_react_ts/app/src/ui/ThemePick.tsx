import { refresh } from "@snapfire/fsr/client";

import { actions } from "@generated/client";

/** Saves a theme, a scheme or both, then redraws from the server's answer. */
export function ThemePick({ theme, scheme, label }: { theme?: string; scheme?: string; label: string }) {
  return (
    <button
      type="button"
      className="button text-[13px]"
      onClick={async () => {
        if (theme !== undefined && scheme !== undefined) await actions.account.setLook({ theme, scheme });
        else if (theme !== undefined) await actions.account.setTheme({ theme });
        else if (scheme !== undefined) await actions.account.setScheme({ scheme });
        await refresh();
      }}
    >
      {label}
    </button>
  );
}
