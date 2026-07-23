import { useState } from 'react';
import { GameEngine } from '@/game/engine';
import { BUILDINGS, CATEGORIES, CATEGORY_NAMES } from '@/game/config';
import type { Category } from '@/game/config';
import { fmtPct } from '@/game/format';
import { GameIcon } from '@/ui/GameIcon';
import { useEngineVersion } from '@/hooks/use-engine';

interface MasterConstructionMenuProps {
  engine: GameEngine;
  onClose: () => void;
  onFocusBuilding?: (id: number) => void;
}

export function MasterConstructionMenu({ engine, onClose, onFocusBuilding }: MasterConstructionMenuProps) {
  useEngineVersion(engine);

  const [activeTab, setActiveTab] = useState<'categories' | 'offices' | 'sites'>('categories');
  const [categoryFilter, setCategoryFilter] = useState<Category | 'all'>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const allBuildings = [...engine.buildings.values()];
  const offices = allBuildings.filter(b => b.constructed && BUILDINGS[b.defId].isConstructionOffice);
  const unconstructedSites = allBuildings.filter(b => !b.constructed);

  const fleet = engine.fleetStatus();

  // Count sites per category
  const siteCountsByCategory: Record<Category, number> = {
    infra: 0,
    housing: 0,
    industry: 0,
    services: 0,
    trade: 0,
  };
  for (const site of unconstructedSites) {
    const cat = BUILDINGS[site.defId].category;
    if (cat in siteCountsByCategory) {
      siteCountsByCategory[cat]++;
    }
  }

  // Filter sites for the directory tab
  const filteredSites = unconstructedSites.filter(site => {
    const def = BUILDINGS[site.defId];
    if (categoryFilter !== 'all' && def.category !== categoryFilter) return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const name = def.name.toLowerCase();
      if (!name.includes(q) && site.id.toString() !== q) return false;
    }
    return true;
  }).sort((a, b) => engine.effectiveBuildPriority(b) - engine.effectiveBuildPriority(a) || a.id - b.id);

  const focus = (id: number) => {
    if (onFocusBuilding) {
      onFocusBuilding(id);
      onClose();
    }
  };

  return (
    <div role="dialog" aria-modal="true" className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm animate-fade-in">
      <div className="flex h-[88vh] w-full max-w-4xl flex-col rounded-lg border border-yellow-600/40 bg-red-950/95 text-yellow-100 shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-yellow-600/30 bg-red-900/60 px-4 py-3">
          <div className="flex items-center gap-2.5">
            <div className="rounded bg-yellow-500/20 p-1.5 text-yellow-400">
              <GameIcon name="builders" size={20} />
            </div>
            <div>
              <h2 className="text-base font-black tracking-wide text-yellow-300 uppercase">
                Master Construction Menu
              </h2>
              <p className="text-xs text-yellow-200/60">
                Global category priorities, fleet management & construction sites
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="rounded p-1.5 text-yellow-200/70 hover:bg-red-800 hover:text-yellow-100 transition-colors"
            title="Close menu"
          >
            &times;
          </button>
        </div>

        {/* Quick Fleet & Master Toggle Bar */}
        <div className="grid grid-cols-3 gap-2 border-b border-yellow-600/20 bg-red-900/30 px-4 py-2 text-xs">
          <div className="flex items-center justify-between rounded bg-red-900/40 px-2.5 py-1.5 border border-yellow-600/20">
            <span className="text-yellow-200/70 flex items-center gap-1.5">
              <GameIcon name="builders" size={13} /> Global Activity:
            </span>
            <button
              onClick={() => engine.setGlobalConstructionEnabled(!engine.globalConstructionEnabled)}
              className={`font-bold px-2 py-0.5 rounded ${
                engine.globalConstructionEnabled
                  ? 'bg-green-600/80 text-white'
                  : 'bg-red-600/80 text-white'
              }`}
            >
              {engine.globalConstructionEnabled ? 'ACTIVE' : 'PAUSED'}
            </button>
          </div>

          <div className="flex items-center justify-between rounded bg-red-900/40 px-2.5 py-1.5 border border-yellow-600/20">
            <span className="text-yellow-200/70 flex items-center gap-1.5">
              <GameIcon name="truck" size={13} /> Fleet in Use:
            </span>
            <span className="font-bold text-yellow-300">
              {fleet.active} / {fleet.max} trucks
            </span>
          </div>

          <div className="flex items-center justify-between rounded bg-red-900/40 px-2.5 py-1.5 border border-yellow-600/20">
            <span className="text-yellow-200/70 flex items-center gap-1.5">
              <GameIcon name="constructionOffice" size={13} /> Ready Sites:
            </span>
            <span className="font-bold text-yellow-300">
              {unconstructedSites.filter(s => !s.paused).length} active ({unconstructedSites.filter(s => s.paused).length} planned)
            </span>
          </div>
        </div>

        {/* Navigation Tabs */}
        <div className="flex border-b border-yellow-600/30 bg-red-900/40 px-4 pt-2 gap-2 text-xs font-bold">
          <button
            onClick={() => setActiveTab('categories')}
            className={`flex items-center gap-1.5 rounded-t px-3.5 py-2 transition-colors ${
              activeTab === 'categories'
                ? 'bg-red-950 border-t-2 border-x border-yellow-500 text-yellow-300 border-b-transparent'
                : 'text-yellow-200/70 hover:bg-red-900/60 hover:text-yellow-100'
            }`}
          >
            <GameIcon name="cat-infra" size={13} /> Global Category Priorities
          </button>
          <button
            onClick={() => setActiveTab('offices')}
            className={`flex items-center gap-1.5 rounded-t px-3.5 py-2 transition-colors ${
              activeTab === 'offices'
                ? 'bg-red-950 border-t-2 border-x border-yellow-500 text-yellow-300 border-b-transparent'
                : 'text-yellow-200/70 hover:bg-red-900/60 hover:text-yellow-100'
            }`}
          >
            <GameIcon name="constructionOffice" size={13} /> Construction Offices ({offices.length})
          </button>
          <button
            onClick={() => setActiveTab('sites')}
            className={`flex items-center gap-1.5 rounded-t px-3.5 py-2 transition-colors ${
              activeTab === 'sites'
                ? 'bg-red-950 border-t-2 border-x border-yellow-500 text-yellow-300 border-b-transparent'
                : 'text-yellow-200/70 hover:bg-red-900/60 hover:text-yellow-100'
            }`}
          >
            <GameIcon name="contract" size={13} /> Site Directory ({unconstructedSites.length})
          </button>
        </div>

        {/* Tab Contents */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {/* TAB 1: Global Category Priorities */}
          {activeTab === 'categories' && (
            <div className="space-y-4">
              <div className="rounded border border-yellow-600/30 bg-red-900/30 p-3 text-xs leading-relaxed text-yellow-200/80">
                <span className="font-bold text-yellow-300">How Category Priorities Work:</span> Global category priorities set the default baseline urgency for all construction sites in that category across the republic. High priority categories receive construction materials and citizen builders first. Sites with an explicit individual priority override will retain their custom priority regardless of category settings.
              </div>

              <div className="grid gap-3">
                {CATEGORIES.map(cat => {
                  const currentPrio = engine.globalCategoryPriorities[cat.id];
                  const count = siteCountsByCategory[cat.id];

                  return (
                    <div
                      key={cat.id}
                      className="flex items-center justify-between rounded-lg border border-yellow-600/30 bg-red-900/40 p-3 hover:border-yellow-500/50 transition-all"
                    >
                      <div className="flex items-center gap-3">
                        <div
                          className="flex h-10 w-10 items-center justify-center rounded-lg border border-yellow-500/30 shadow-inner"
                          style={{ backgroundColor: `${cat.accent}25`, color: cat.accent }}
                        >
                          <GameIcon name={cat.icon} size={20} />
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <h3 className="font-bold text-sm text-yellow-200">{cat.name}</h3>
                            <span className="rounded bg-red-950/80 px-2 py-0.5 text-[0.625rem] font-semibold text-yellow-200/60 border border-yellow-600/20">
                              {count} active/planned site{count === 1 ? '' : 's'}
                            </span>
                          </div>
                          <p className="text-xs text-yellow-200/50">
                            {cat.id === 'infra' && 'Roads, bridges, and construction facilities'}
                            {cat.id === 'housing' && 'Residential apartments and family homes'}
                            {cat.id === 'industry' && 'Mines, mills, farms, power plants, and factories'}
                            {cat.id === 'services' && 'Shops, polyclinics, culture clubs'}
                            {cat.id === 'trade' && 'Warehouses, motor depots, customs, ports'}
                          </p>
                        </div>
                      </div>

                      {/* Priority Controls */}
                      <div className="flex items-center gap-1.5 bg-red-950/80 p-1 rounded-lg border border-yellow-600/30">
                        <button
                          onClick={() => engine.setGlobalCategoryPriority(cat.id, -1)}
                          className={`px-3 py-1.5 rounded text-xs font-bold transition-all ${
                            currentPrio === -1
                              ? 'bg-blue-600 text-white shadow'
                              : 'text-yellow-200/60 hover:text-yellow-100 hover:bg-red-800/50'
                          }`}
                        >
                          Low
                        </button>
                        <button
                          onClick={() => engine.setGlobalCategoryPriority(cat.id, 0)}
                          className={`px-3 py-1.5 rounded text-xs font-bold transition-all ${
                            currentPrio === 0
                              ? 'bg-yellow-500 text-red-950 shadow'
                              : 'text-yellow-200/60 hover:text-yellow-100 hover:bg-red-800/50'
                          }`}
                        >
                          Normal
                        </button>
                        <button
                          onClick={() => engine.setGlobalCategoryPriority(cat.id, 1)}
                          className={`px-3 py-1.5 rounded text-xs font-bold transition-all ${
                            currentPrio === 1
                              ? 'bg-red-600 text-white shadow animate-pulse'
                              : 'text-yellow-200/60 hover:text-yellow-100 hover:bg-red-800/50'
                          }`}
                        >
                          High
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* TAB 2: Construction Offices */}
          {activeTab === 'offices' && (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-3 text-xs">
                <div className="rounded border border-yellow-600/30 bg-red-900/40 p-3 space-y-1">
                  <div className="font-bold text-yellow-300">Construction Office Base Capacity</div>
                  <div className="text-yellow-200/70">
                    Construction Offices supply hauling trucks and organize citizen builder crews. Their vehicles consume network fuel like every other truck.
                  </div>
                  <div className="pt-1 font-bold text-yellow-200">
                    Total CO Fleet: {fleet.officeTrucks} trucks
                  </div>
                </div>
                <div className="rounded border border-yellow-600/30 bg-red-900/40 p-3 space-y-1">
                  <div className="font-bold text-yellow-300">Motor Depot Fleet Top-Up</div>
                  <div className="text-yellow-200/70">
                    Motor Depots provide extra hauling capacity. The whole fleet shares fuel held by any connected building that can store it.
                  </div>
                  <div className="pt-1 font-bold text-yellow-200">
                    Depot Trucks: {fleet.driverTrucks} · Fuelled Fleet: {fleet.max}/{fleet.officeTrucks + fleet.driverTrucks}
                  </div>
                </div>
              </div>

              {offices.length === 0 ? (
                <div className="rounded border border-yellow-600/20 bg-red-900/20 p-8 text-center text-yellow-200/60 text-xs">
                  No constructed Construction Offices found in the republic. Build one from the Infrastructure menu to speed up construction!
                </div>
              ) : (
                <div className="grid gap-3">
                  {offices.map(office => {
                    const def = BUILDINGS[office.defId];
                    const trucks = engine.trucksFrom(office);
                    return (
                      <div
                        key={office.id}
                        className="flex items-center justify-between rounded-lg border border-yellow-600/30 bg-red-900/40 p-3"
                      >
                        <div className="flex items-center gap-3">
                          <div className="rounded-lg bg-yellow-500/20 p-2 text-yellow-400 border border-yellow-500/30">
                            <GameIcon name="constructionOffice" size={20} />
                          </div>
                          <div>
                            <div className="font-bold text-yellow-200 text-sm flex items-center gap-2">
                              Construction Office #{office.id}
                              <span className="text-xs font-normal text-yellow-200/50">({office.x}, {office.y})</span>
                              {!office.connected && (
                                <span className="rounded bg-red-600/80 px-1.5 py-0.5 text-[0.625rem] text-white">
                                  Not connected to road
                                </span>
                              )}
                            </div>
                            <div className="flex items-center gap-3 text-xs text-yellow-200/70 mt-0.5">
                              <span>Staff: <strong className="text-yellow-300">{office.staff}/{def.workers}</strong></span>
                              <span>Efficiency: <strong className="text-yellow-300">{fmtPct(office.eff)}%</strong></span>
                              <span>Fleet: <strong className="text-yellow-300">{trucks} trucks</strong></span>
                            </div>
                          </div>
                        </div>

                        {onFocusBuilding && (
                          <button
                            onClick={() => focus(office.id)}
                            className="flex items-center gap-1 rounded border border-yellow-600/40 bg-red-900/60 px-3 py-1.5 text-xs font-bold text-yellow-200 hover:bg-yellow-500 hover:text-red-950 transition-colors"
                          >
                            <GameIcon name="search" size={12} /> View Location
                          </button>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {/* TAB 3: Site Directory */}
          {activeTab === 'sites' && (
            <div className="space-y-3">
              {/* Controls Header */}
              <div className="flex flex-wrap items-center justify-between gap-2 rounded bg-red-900/40 p-2 border border-yellow-600/20 text-xs">
                <div className="flex items-center gap-2 flex-1 min-w-[200px]">
                  <input
                    type="text"
                    placeholder="Filter by building name or ID..."
                    value={searchQuery}
                    onChange={e => setSearchQuery(e.target.value)}
                    className="w-full rounded border border-yellow-600/40 bg-red-950 px-2.5 py-1 text-xs text-yellow-100 placeholder-yellow-200/40 focus:outline-none focus:border-yellow-400"
                  />
                </div>

                <div className="flex items-center gap-1.5">
                  <select
                    value={categoryFilter}
                    onChange={e => setCategoryFilter(e.target.value as Category | 'all')}
                    className="rounded border border-yellow-600/40 bg-red-950 px-2 py-1 text-xs text-yellow-200 focus:outline-none focus:border-yellow-400"
                  >
                    <option value="all">All Categories</option>
                    {CATEGORIES.map(c => (
                      <option key={c.id} value={c.id}>{c.name}</option>
                    ))}
                  </select>

                  <button
                    onClick={() => engine.commenceAllPlanned()}
                    className="rounded border border-green-600/50 bg-green-900/40 px-2.5 py-1 font-bold text-green-200 hover:bg-green-800/60"
                    title="Commence all planned sites treasury can afford"
                  >
                    Commence All Planned
                  </button>
                </div>
              </div>

              {/* Site Table / List */}
              {filteredSites.length === 0 ? (
                <div className="rounded border border-yellow-600/20 bg-red-900/20 p-8 text-center text-yellow-200/60 text-xs">
                  No construction sites match the selected filters.
                </div>
              ) : (
                <div className="space-y-2">
                  {filteredSites.map(site => {
                    const def = BUILDINGS[site.defId];
                    const effPrio = engine.effectiveBuildPriority(site);
                    const isOverridden = site.buildPriority !== undefined;
                    const catDef = CATEGORIES.find(c => c.id === def.category);
                    const pct = Math.min(100, Math.floor((site.progress / def.labor) * 100));

                    return (
                      <div
                        key={site.id}
                        className="flex flex-col sm:flex-row sm:items-center justify-between rounded border border-yellow-600/30 bg-red-900/40 p-2.5 gap-2"
                      >
                        <div className="flex items-center gap-3">
                          <div
                            className="flex h-8 w-8 items-center justify-center rounded border border-yellow-500/20 shrink-0"
                            style={{ backgroundColor: `${catDef?.accent ?? '#ffffff'}25`, color: catDef?.accent }}
                          >
                            <GameIcon name={def.icon} size={16} />
                          </div>
                          <div>
                            <div className="flex items-center gap-2 font-bold text-xs text-yellow-200">
                              <span>{def.name} #{site.id}</span>
                              <span className="text-[0.625rem] font-normal text-yellow-200/50">({site.x}, {site.y})</span>
                              {site.paused && (
                                <span className="rounded bg-yellow-900/80 px-1.5 py-0.5 text-[0.625rem] text-yellow-300 border border-yellow-600/30">
                                  PLANNED
                                </span>
                              )}
                            </div>
                            <div className="flex items-center gap-3 text-[0.6875rem] text-yellow-200/60 mt-0.5">
                              <span>Category: <strong>{CATEGORY_NAMES[def.category]}</strong></span>
                              <span>Progress: <strong>{pct}%</strong></span>
                            </div>
                          </div>
                        </div>

                        {/* Right side controls */}
                        <div className="flex items-center gap-2 shrink-0">
                          {/* Priority Badge & Selector */}
                          <div className="flex items-center gap-1">
                            <span className="text-[0.625rem] text-yellow-200/50">
                              {isOverridden ? 'Local Override:' : 'Inherited:'}
                            </span>
                            <div className="flex items-center rounded bg-red-950 border border-yellow-600/30 text-[0.6875rem] font-bold overflow-hidden">
                              <button
                                onClick={() => engine.setSitePriority(site.id, -1)}
                                className={`px-2 py-0.5 ${effPrio === -1 ? 'bg-blue-600 text-white' : 'text-yellow-200/60 hover:text-yellow-100'}`}
                                title="Low Priority"
                              >
                                L
                              </button>
                              <button
                                onClick={() => engine.setSitePriority(site.id, 0)}
                                className={`px-2 py-0.5 ${effPrio === 0 ? 'bg-yellow-500 text-red-950' : 'text-yellow-200/60 hover:text-yellow-100'}`}
                                title="Normal Priority"
                              >
                                N
                              </button>
                              <button
                                onClick={() => engine.setSitePriority(site.id, 1)}
                                className={`px-2 py-0.5 ${effPrio === 1 ? 'bg-red-600 text-white' : 'text-yellow-200/60 hover:text-yellow-100'}`}
                                title="High Priority"
                              >
                                H
                              </button>
                            </div>
                            {isOverridden && (
                              <button
                                onClick={() => engine.setSitePriority(site.id, undefined)}
                                className="text-[0.625rem] text-yellow-400 underline hover:text-yellow-200 ml-1"
                                title="Reset to global category priority default"
                              >
                                Reset
                              </button>
                            )}
                          </div>

                          {/* Pause/Resume button */}
                          <button
                            onClick={() => engine.setSitePaused(site.id, !site.paused)}
                            className={`px-2 py-1 rounded text-xs font-bold border ${
                              site.paused
                                ? 'border-green-600/40 bg-green-900/40 text-green-200 hover:bg-green-800'
                                : 'border-yellow-600/30 bg-red-900/50 text-yellow-200 hover:bg-red-800'
                            }`}
                          >
                            {site.paused ? 'Commence' : 'Pause'}
                          </button>

                          {onFocusBuilding && (
                            <button
                              onClick={() => focus(site.id)}
                              className="p-1 rounded border border-yellow-600/30 bg-red-900/40 text-yellow-200 hover:bg-yellow-500 hover:text-red-950"
                              title="Focus site on map"
                            >
                              <GameIcon name="search" size={13} />
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
