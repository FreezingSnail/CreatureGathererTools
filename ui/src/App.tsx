import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [pong, setPong] = useState<string>("loading…");

  useEffect(() => {
    invoke<string>("ping")
      .then((res) => setPong(res))
      .catch((err) => setPong(`error: ${err}`));
  }, []);

  return (
    <main>
      <h1>CreatureGathererFX</h1>
      <p>ping → {pong}</p>
    </main>
  );
}

export default App;