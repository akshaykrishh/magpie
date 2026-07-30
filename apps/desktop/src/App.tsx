import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

const HOTKEY = "CommandOrControl+Shift+M";

function App() {
  const [fireCount, setFireCount] = useState(0);

  useEffect(() => {
    const unlisten = listen("toast:fired", () => setFireCount((n) => n + 1));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <main className="container">
      <h1>M0 focus spike</h1>
      <p>
        This proves the toast window can appear on a global hotkey without
        stealing keyboard focus from whatever app you're typing in.
      </p>

      <ol style={{ textAlign: "left", maxWidth: 480, margin: "0 auto" }}>
        <li>
          Click into the text field below (or switch to any other app —
          TextEdit, a terminal, Notes).
        </li>
        <li>
          Start typing continuously, e.g. hold down a letter key or type a
          sentence slowly.
        </li>
        <li>
          While typing, without pausing, press <kbd>{HOTKEY}</kbd>.
        </li>
        <li>
          A toast should appear elsewhere on screen. Your typing should
          <strong> never be interrupted</strong> and the field below should
          keep receiving keystrokes.
        </li>
      </ol>

      <textarea
        rows={6}
        style={{ width: "100%", maxWidth: 480, fontSize: 16 }}
        placeholder="Type here while pressing the hotkey elsewhere..."
        autoFocus
      />

      <p>
        Toast fired <strong>{fireCount}</strong> time{fireCount === 1 ? "" : "s"}.
      </p>
      <p style={{ opacity: 0.6, fontSize: 13 }}>
        If the toast window ever steals OS focus, its title bar (visible in
        Cmd+Tab / the dock) will change to "FOCUS STOLEN" and an error is
        logged to its devtools console — that's the failure signal to watch
        for, on top of your typing simply not stopping.
      </p>
    </main>
  );
}

export default App;
