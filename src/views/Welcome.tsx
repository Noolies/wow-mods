import { Mark, Ridgeline } from "../components/Art";
import { GameFinder } from "../components/GameFinder";

export function Welcome() {
  return (
    <div className="welcome">
      <Ridgeline className="game-header__art" />
      <div className="welcome__card">
        <Mark className="welcome__mark" />
        <h1 className="welcome__title">Hearthforge</h1>
        <p className="welcome__lede">
          Addons for WoW Forever, installed in one click and kept up to date. No ads, no account, nothing running you
          didn't ask for.
        </p>
        <div style={{ display: "inline-block", minWidth: 420, textAlign: "left" }}>
          <GameFinder autoScan />
        </div>
      </div>
    </div>
  );
}
