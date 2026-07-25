// Who can be reached by freight. `connected` = a finite-cost land-or-road path
// reaches the depot network (gates staffing, builders and lorry ownership);
// `roadConnected` is the stricter road-only state behind the "off-road — slow;
// lay a road" advisory.
//
// Expensive and rarely changing, so the result is cached on the World until the
// road/land topology or the facility set moves. Re-applying a cached list is
// harmless: every mutation is an absolute set, not a delta.
import type { Mutation } from '../mutation';
import type { World } from '../world';
import { shareAnyComponent, unionComponents } from '../topology';

export function connectivity(w: World): Mutation[] {
  return w.connectivityCache.get(w.connectivityDeps(), () => compute(w));
}

function compute(w: World): Mutation[] {
  // A building participates if ANY component touched by its access perimeter
  // also touches the freight network. With no hub at all, preserve the
  // historical fallback: any local access tile counts as connected.
  //
  // Ports seed the network as well as depots. A port IS a freight hub — it is
  // where barges land goods — and now that lorries are physical, a building
  // that is not `connected` owns no fleet. Without this an island served by
  // barge could never unload them: nothing over there would be connected, so no
  // garage there would have a single lorry.
  const depots = [...w.buildings.values()].filter(b => {
    const def = w.def(b);
    return (def.isDepot || def.isPort) && b.constructed;
  });
  const roadComponents = unionComponents(...depots.map(d => w.roadAccess(d).components));
  const landComponents = unionComponents(...depots.map(d => w.landAccess(d).components));
  const out: Mutation[] = [];
  for (const b of w.buildings.values()) {
    const road = w.roadAccess(b);
    const land = w.landAccess(b);
    out.push({
      k: 'connectivity',
      id: b.id,
      roadConnected: road.tiles.length > 0 &&
        (!depots.length || shareAnyComponent(road.components, roadComponents)),
      connected: land.tiles.length > 0 &&
        (!depots.length || shareAnyComponent(land.components, landComponents)),
    });
  }
  return out;
}
