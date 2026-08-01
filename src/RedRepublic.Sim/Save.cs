using System.Buffers.Binary;

namespace RedRepublic.Sim;

/// <summary>
/// The save format, owned by the simulation.
/// </summary>
/// <remarks>
/// <para>
/// <b>It must round-trip every <c>double</c> bit-exactly</b>, which disqualifies
/// JSON — this repository measured a JSON library returning a different value
/// for 91,767 of 200,000 sampled doubles, because its parser is not correctly
/// rounded even when the digits it wrote were right. A republic reloaded one ulp
/// out is a republic that diverges from the one it was, and nothing downstream
/// would ever say so. So: raw little-endian bits, written and read by this file.
/// </para>
/// <para>
/// <b>The crate owns the format.</b> A save handed out as a serialisable object
/// is a format the caller picks, and a caller that reached for JSON would
/// reintroduce exactly the corruption this requirement exists to prevent — with
/// the guard still passing, because it would be testing a format nothing uses.
/// </para>
/// <para>
/// <b>The generator's stream position goes in, not its seed.</b> A save that
/// restores the seed alone resumes a different future, and the failure is
/// invisible until somebody compares two runs.
/// </para>
/// </remarks>
public static class Save
{
    /// <summary>
    /// What the file starts with, so a wrong file is refused rather than
    /// misread. "RRSV" and a version that is checked rather than assumed.
    /// </summary>
    private static ReadOnlySpan<byte> Magic => "RRSV"u8;

    /// <summary>
    /// The format version.
    /// </summary>
    /// <remarks>
    /// Bumped whenever the layout changes. A save from a different version is
    /// refused with a sentence rather than read as garbage — the failure mode
    /// that costs a player their republic is the one that half-works.
    /// </remarks>
    public const int Version = 3;

    public static byte[] Write(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var w = new Writer();

        w.Bytes(Magic);
        w.Int(Version);

        // Which balance table this republic was played against.
        //
        // Every figure in the file is a consequence of it, so a manifest edited
        // and re-stamped since silently shifts the ground under every save the
        // player already has: the same seed, the same journal and a different
        // republic. The version number cannot see that — the layout is
        // unchanged — so the table's own checksum goes in beside it.
        w.String(world.Tables.ChecksumGot);

        // What the world was founded on, so the terrain and geology can be
        // regenerated rather than stored. A million cells of height is thirty
        // times the size of everything else in the file, and it is a pure
        // function of the seed.
        w.ULong(world.Spec.Seed);
        w.Double(world.Spec.Extent);
        w.Int(world.Spec.Climate);
        w.String(world.Name);
        w.Long(world.Clock.Ticks);

        var rng = world.Rng.State;
        w.ULong(rng.S0);
        w.ULong(rng.S1);
        w.ULong(rng.S2);
        w.ULong(rng.S3);

        w.Double(world.Ground.Moisture);
        w.Double(world.Ground.Water);
        w.Double(world.Ground.Snow);
        w.Double(world.Ground.Frost);

        // What is left in the ground, and what the republic has done to the
        // surface of it.
        //
        // <b>Both are regenerated from the seed on load, and both are wrong.</b>
        // A seam mined half out came back full, a corridor worn into a track
        // came back virgin, a valley under a decade of smoke came back clean,
        // and a January reload ploughed the whole map. Exhaustion is the
        // longest-running pressure in the economy and it could not survive a
        // session boundary — which is a save-scum exploit against the one
        // thing the decade is meant to be a fight with.
        //
        // Layer by layer rather than as a total: a deposit is worked from the
        // top down and where the working face is decides what the next tonne
        // costs.
        w.Int(world.Geology.All.Count);
        foreach (var deposit in world.Geology.All)
        {
            w.Int(deposit.Id);
            w.Int(deposit.Layers.Length);
            foreach (var layer in deposit.Layers)
            {
                w.Double(layer.Remaining);
            }
        }

        w.Int(world.Lattice.Cells);
        for (var cell = 0; cell < world.Lattice.Cells * world.Lattice.Cells; cell++)
        {
            w.Double(world.Lattice.WearAt(cell));
            w.Double(world.Lattice.PollutionAt(cell));
            w.Double(world.Lattice.ClearedAt(cell));
        }

        w.Double(world.Treasury.Of(Market.East));
        w.Double(world.Treasury.Of(Market.West));

        var b = world.Buildings;
        w.Int(b.Count);
        for (var i = 0; i < b.Count; i++)
        {
            w.Int(b.IdAt(i));
            w.Int(b.KindAt(i));
            w.Double(b.XAt(i));
            w.Double(b.YAt(i));
            w.Int((int)b.PriorityAt(i));
            w.Int(b.StaffAt(i));
            w.Bool(b.PoweredAt(i));
            w.Bool(b.HeatedAt(i));
            w.Double(b.WorkDoneAt(i));
            w.Int(b.ContractorAt(i));
            w.Int(b.TappedAt(i));
            w.Int(b.ShiftsAt(i));
            w.Double(b.HoursAt(i));
            w.Double(b.ProvisionedAt(i));
            w.Double(b.ComfortedAt(i));
            w.Double(b.DrinkAt(i));

            for (var r = 0; r < world.Tables.Resources.Length; r++)
            {
                w.Double(b.Stock.Get(i, r));
                w.Double(b.Orders.Get(i, r));
            }
        }

        // The ways. Open roads go in as the runs that made them rather than as
        // junctions and segments, and are re-laid on load — junction merging is
        // order-dependent, so replaying the openings in order reproduces the
        // graph exactly and storing the graph would not reproduce the merging.
        w.Int(world.Roadbook.Count);
        foreach (var r in world.Roadbook)
        {
            w.Double(r.FromX);
            w.Double(r.FromY);
            w.Double(r.ToX);
            w.Double(r.ToY);
            w.Int(r.Grade);
            w.Bool(r.Lamps);
        }

        w.Int(world.RoadWorks.Sites.Count);
        foreach (var s in world.RoadWorks.Sites)
        {
            w.Int(s.Id);
            w.Double(s.FromX);
            w.Double(s.FromY);
            w.Double(s.ToX);
            w.Double(s.ToY);
            w.Int(s.Grade);
            w.Bool(s.Lamps);
            w.Long(s.Ordered);
            w.Double(s.WorkDone);

            var i = world.RoadWorks.IndexOf(s.Id);
            for (var res = 0; res < world.Tables.Resources.Length; res++)
            {
                w.Double(world.RoadWorks.Stock.Get(i, res));
            }
        }

        // The lines. Energised spans go in as geometry in the order they were
        // built, and the grid is rebuilt from them on load rather than stored —
        // the union-find and every attachment are a pure function of that order
        // and of where the buildings stand, which are both already in the file.
        // The same reasoning that keeps a million cells of terrain out of it.
        var lines = world.Grid.Lines;
        w.Int(lines.Count);
        foreach (var l in lines)
        {
            w.Int(l.Kind);
            w.Double(l.FromX);
            w.Double(l.FromY);
            w.Double(l.ToX);
            w.Double(l.ToY);
        }

        w.Int(world.LineWorks.Sites.Count);
        foreach (var s in world.LineWorks.Sites)
        {
            w.Int(s.Id);
            w.Int(s.Kind);
            w.Double(s.FromX);
            w.Double(s.FromY);
            w.Double(s.ToX);
            w.Double(s.ToY);
            w.Long(s.Ordered);
            w.Double(s.WorkDone);

            var i = world.LineWorks.IndexOf(s.Id);
            for (var res = 0; res < world.Tables.Resources.Length; res++)
            {
                w.Double(world.LineWorks.Stock.Get(i, res));
            }
        }

        var p = world.Citizens;
        w.Int(p.Count);
        for (var i = 0; i < p.Count; i++)
        {
            w.Int(p.IdAt(i));
            w.Int(p.HomeAt(i));
            w.Int(p.WorkplaceAt(i));
            w.Int(p.AgeAt(i));
            w.Int(p.SchoolDaysAt(i));
            w.Bool(p.StudyingAt(i));
            w.Double(p.HealthAt(i));
            w.Double(p.LoyaltyAt(i));
        }

        // The fleet, mid-journey and all. A vehicle halfway to a site is state
        // rather than a decision, so the journey goes in as it stands: a
        // re-planned one would arrive at a different tick and the republic would
        // quietly diverge from the one that was written.
        var f = world.Fleet;
        w.Int(f.Count);
        for (var i = 0; i < f.Count; i++)
        {
            w.Int(f.IdAt(i));
            w.Int(f.KindAt(i));
            w.Int(f.HomeAt(i));
            w.Int((int)f.StateAt(i));
            w.Int((int)f.DoingAt(i));
            w.Long(f.BoggedSinceAt(i));
            w.Double(f.XAt(i));
            w.Double(f.YAt(i));
            w.Double(f.FuelAt(i));

            var job = f.JobAt(i);
            w.Int((int)job.Kind);
            w.Int(job.From);
            w.Int((int)job.To.Kind);
            w.Int(job.To.Id);
            w.Double(job.To.X);
            w.Double(job.To.Y);
            w.Int(job.Resource);
            w.Double(job.Tonnes);
            w.Int(job.Subject);

            var journey = f.JourneyAt(i);
            w.Int(journey?.PathX.Count ?? 0);
            if (journey is not null)
            {
                for (var leg = 0; leg < journey.PathX.Count; leg++)
                {
                    w.Double(journey.PathX[leg]);
                    w.Double(journey.PathY[leg]);
                }

                foreach (var limit in journey.Limits)
                {
                    w.Double(limit);
                }

                w.Int(journey.Leg);
                w.Double(journey.LegStart);
                w.Double(journey.LegEnd);
            }

            for (var res = 0; res < world.Tables.Resources.Length; res++)
            {
                w.Double(f.Cargo.Get(i, res));
            }
        }

        // The gangs, where they stand and what they are on.
        w.Int(world.Crews.All.Count);
        foreach (var party in world.Crews.All)
        {
            w.Int(party.Id);
            w.Int(party.Office);
            w.Int(party.Heads);
            w.Double(party.X);
            w.Double(party.Y);
            w.Int(party.HiredFrom is null ? -1 : (int)party.HiredFrom.Value);
            w.Int(party.Working is null ? -1 : (int)party.Working.Value.Kind);
            w.Int(party.Working?.Id ?? -1);
            w.Int(party.Riding ?? -1);
        }

        w.Int(world.Crews.HiredByOffice.Count);
        foreach (var (office, heads) in world.Crews.HiredByOffice)
        {
            w.Int(office);
            w.Int(heads);

            // Which bloc they came from, read off a party of theirs. A count with
            // no bloc would be a wage bill in no currency.
            var bloc = Market.East;
            foreach (var party in world.Crews.OfOffice(office))
            {
                if (party.HiredFrom is not null)
                {
                    bloc = party.HiredFrom.Value;
                    break;
                }
            }

            w.Int((int)bloc);
        }

        // People still on their way in, and visitors part-way through a stay.
        w.Int(world.Migration.Waiting.Count);
        foreach (var g in world.Migration.Waiting)
        {
            w.Int(g.Id);
            w.Double(g.X);
            w.Double(g.Y);
            w.Int(g.Heads);
            w.Long(g.Since);
            w.Int(g.Riding ?? -1);
        }

        w.Int(world.Tourism.Visits.Count);
        foreach (var v in world.Tourism.Visits)
        {
            w.Int(v.Id);
            w.Double(v.X);
            w.Double(v.Y);
            w.Int(v.Heads);
            w.Int((int)v.Market);
            w.Long(v.Since);
            w.Long(v.Until);
            w.Int(v.Riding ?? -1);
            w.Int(v.StayingAt ?? -1);
        }

        // What is owed, and to whom the republic can no longer go.
        w.Int(world.Loans.All.Count);
        foreach (var l in world.Loans.All)
        {
            w.Int((int)l.Market);
            w.Double(l.Principal);
            w.Double(l.Owed);
            w.Double(l.Repaid);
            w.Long(l.TakenDay);
            w.Long(l.DueDay);
        }

        w.Int(world.Loans.Burnt.Count);
        foreach (var market in world.Loans.Burnt)
        {
            w.Int((int)market);
        }

        w.Int(world.Loans.Defaulted);
        w.Int(world.Loans.Cleared);

        // The tenders, offered, taken and closed — and how each bloc feels.
        w.Int(world.Contracts.All.Count);
        foreach (var c in world.Contracts.All)
        {
            w.Int(c.Id);
            w.Int((int)c.Market);
            w.Int(c.Resource);
            w.Double(c.Tonnes);
            w.Double(c.PricePerTonne);
            w.Long(c.OfferedDay);
            w.Long(c.ExpiresDay);
            w.Long(c.DeadlineDay);
            w.Int((int)c.State);
            w.Double(c.Delivered);
        }

        w.Double(world.Contracts.Relations(Market.East));
        w.Double(world.Contracts.Relations(Market.West));

        // The standing instructions to the customs houses, in their order,
        // because the order is the decision.
        w.Int(world.TradeRules.Count);
        foreach (var rule in world.TradeRules)
        {
            w.Int(rule.Resource);
            w.Int((int)rule.Market);
            w.Int((int)rule.Action);
        }

        // Where sites buy what the republic has not made.
        w.Int(world.BuildPolicy.Global ?? -1);
        w.Int(world.BuildPolicy.Sites.Count);
        foreach (var (site, crossing) in world.BuildPolicy.Sites)
        {
            w.Int((int)site.Kind);
            w.Int(site.Id);
            w.Double(site.X);
            w.Double(site.Y);
            w.Int(crossing ?? -1);
        }

        // The working day, and every exception to it.
        w.Double(world.Buildings.Shifts.National);
        w.Int(world.Buildings.Shifts.KindRules.Count);
        foreach (var (kind, hours) in world.Buildings.Shifts.KindRules)
        {
            w.Int(kind);
            w.Double(hours);
        }

        // The journal, which is how the republic came to be. A save records its
        // own history, so a replay is possible at all.
        w.Int(world.Journal.Count);
        foreach (var (tick, command) in world.Journal.Entries)
        {
            w.Long(tick);
            w.Int((int)command.Kind);
            w.Int(command.A);
            w.Int(command.B);
            w.Int(command.C);
            w.Double(command.X);
            w.Double(command.Y);
            w.Double(command.ToX);
            w.Double(command.ToY);
            w.Double(command.Amount);
            w.Bool(command.Flag);
            w.String(command.Text);
        }

        return w.ToArray();
    }

    /// <summary>
    /// Read a republic back.
    /// </summary>
    /// <exception cref="InvalidDataException">
    /// If the file is not a save, or is a version this build cannot read.
    /// </exception>
    public static World Read(byte[] bytes, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        ArgumentNullException.ThrowIfNull(tables);
        var r = new Reader(bytes);

        if (!r.Bytes(4).SequenceEqual(Magic))
        {
            throw new InvalidDataException("that is not a Red Republic save");
        }

        var version = r.Int();
        if (version != Version)
        {
            throw new InvalidDataException(
                $"that save was written by version {version} and this build reads {Version}");
        }

        var against = r.String();
        if (!string.Equals(against, tables.ChecksumGot, StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                "that republic was played against a different balance table "
                + $"({against}); this build carries {tables.ChecksumGot}");
        }

        var seed = r.ULong();
        var extent = r.Double();
        var climate = r.Int();
        var world = World.Found(new WorldSpec(seed, extent, climate), tables);

        world.Name = r.String();
        world.Clock.AdvanceBy(r.Long());
        world.Rng.State = new RngState(r.ULong(), r.ULong(), r.ULong(), r.ULong());

        world.Ground = new Ground
        {
            Moisture = r.Double(),
            Water = r.Double(),
            Snow = r.Double(),
            Frost = r.Double(),
        };

        var deposits = r.Int();
        for (var i = 0; i < deposits; i++)
        {
            var deposit = world.Geology.Get(r.Int());
            var layers = r.Int();
            for (var layer = 0; layer < layers; layer++)
            {
                var left = r.Double();
                if (deposit is not null && layer < deposit.Layers.Length)
                {
                    deposit.Layers[layer].Remaining = left;
                }
            }
        }

        var cells = r.Int();
        if (cells != world.Lattice.Cells)
        {
            throw new InvalidDataException(
                $"that save carries a {cells}-cell lattice and this map has "
                + $"{world.Lattice.Cells}");
        }

        for (var cell = 0; cell < cells * cells; cell++)
        {
            world.Lattice.Restore(cell, r.Double(), r.Double(), r.Double());
        }

        world.Treasury.Set(Market.East, r.Double());
        world.Treasury.Set(Market.West, r.Double());

        var buildings = r.Int();
        for (var i = 0; i < buildings; i++)
        {
            var id = r.Int();
            var kind = r.Int();
            var x = r.Double();
            var y = r.Double();
            var b = world.Buildings.Restore(id, kind, x, y);

            world.Buildings.SetPriority(id, (Priority)r.Int());
            world.Buildings.SetStaff(b, r.Int());
            world.Buildings.SetPowered(b, r.Bool());
            world.Buildings.SetHeated(b, r.Bool());
            world.Buildings.AddWork(b, r.Double());
            world.Buildings.SetContractor(b, r.Int());
            world.Buildings.SetTapped(b, r.Int());
            world.Buildings.SetShiftCount(id, r.Int());
            world.Buildings.SetBuildingHours(id, r.Double());
            world.Buildings.SetProvisioned(b, r.Double());
            world.Buildings.SetComforted(b, r.Double());
            world.Buildings.SetDrink(b, r.Double());

            for (var res = 0; res < tables.Resources.Length; res++)
            {
                world.Buildings.Stock.Set(b, res, r.Double());
                world.Buildings.Orders.Set(b, res, r.Double());
            }
        }

        // Re-lay in the order they were opened, so the junction merging lands
        // exactly where it did the first time.
        var roads = r.Int();
        for (var i = 0; i < roads; i++)
        {
            var run = new RoadSite(
                0, r.Double(), r.Double(), r.Double(), r.Double(), r.Int(), r.Bool(), 0);
            world.Reopen(run);
        }

        var roadSites = r.Int();
        for (var i = 0; i < roadSites; i++)
        {
            var id = r.Int();
            world.RoadWorks.Restore(
                id, r.Double(), r.Double(), r.Double(), r.Double(),
                r.Int(), r.Bool(), r.Long(), r.Double());

            var at = world.RoadWorks.IndexOf(id);
            for (var res = 0; res < tables.Resources.Length; res++)
            {
                world.RoadWorks.Stock.Set(at, res, r.Double());
            }
        }

        // Re-energise in the order they were built, then plug everything in.
        // Both are order-dependent, and both orders are in the file.
        var lines = r.Int();
        for (var i = 0; i < lines; i++)
        {
            var kind = r.Int();
            var site = new LineSite(0, kind, r.Double(), r.Double(), r.Double(), r.Double(), 0);
            world.Grid.Energise(site);
        }

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            world.Grid.AttachAll(
                world.Buildings.IdAt(b), world.Buildings.XAt(b), world.Buildings.YAt(b));
        }

        var lineSites = r.Int();
        for (var i = 0; i < lineSites; i++)
        {
            var id = r.Int();
            var kind = r.Int();
            world.LineWorks.Restore(
                id, kind, r.Double(), r.Double(), r.Double(), r.Double(), r.Long(), r.Double());

            var at = world.LineWorks.IndexOf(id);
            for (var res = 0; res < tables.Resources.Length; res++)
            {
                world.LineWorks.Stock.Set(at, res, r.Double());
            }
        }

        var citizens = r.Int();
        for (var i = 0; i < citizens; i++)
        {
            var id = r.Int();
            var home = r.Int();
            var workplace = r.Int();
            var age = r.Int();
            var schoolDays = r.Int();
            var studying = r.Bool();
            var health = r.Double();
            var loyalty = r.Double();

            var c = world.Citizens.Restore(id, home, age, schoolDays, health, loyalty);
            world.Citizens.SetStudying(c, studying);
            if (workplace >= 0)
            {
                world.Citizens.SetWorkplace(c, workplace, Commute.None);
            }
        }

        var vehicles = r.Int();
        for (var i = 0; i < vehicles; i++)
        {
            var id = r.Int();
            var kind = r.Int();
            var home = r.Int();
            var state = (VehicleState)r.Int();
            var doing = (VehicleState)r.Int();
            var bogged = r.Long();
            var x = r.Double();
            var y = r.Double();
            var fuel = r.Double();

            var job = new Job(
                (JobKind)r.Int(), r.Int(),
                new Destination((DestinationKind)r.Int(), r.Int(), r.Double(), r.Double()),
                r.Int(), r.Double(), r.Int());

            Journey? journey = null;
            var points = r.Int();
            if (points > 0)
            {
                var px = new double[points];
                var py = new double[points];
                for (var p = 0; p < points; p++)
                {
                    px[p] = r.Double();
                    py[p] = r.Double();
                }

                var limits = new double[points - 1];
                for (var p = 0; p < limits.Length; p++)
                {
                    limits[p] = r.Double();
                }

                journey = Journey.Restore(px, py, limits, r.Int(), r.Double(), r.Double());
            }

            var v = world.Fleet.Restore(
                id, kind, home, state, doing, bogged, x, y, fuel, job, journey);

            for (var res = 0; res < tables.Resources.Length; res++)
            {
                world.Fleet.Cargo.Set(v, res, r.Double());
            }
        }

        var parties = r.Int();
        for (var i = 0; i < parties; i++)
        {
            var id = r.Int();
            var office = r.Int();
            var heads = r.Int();
            var x = r.Double();
            var y = r.Double();
            var from = r.Int();
            var workingKind = r.Int();
            var workingId = r.Int();
            var riding = r.Int();

            world.Crews.Restore(
                id, office, heads, x, y,
                from < 0 ? null : (Market)from,
                workingKind < 0
                    ? null
                    : new Destination((DestinationKind)workingKind, workingId, 0.0, 0.0),
                riding < 0 ? null : riding);
        }

        var hired = r.Int();
        for (var i = 0; i < hired; i++)
        {
            world.Crews.RestoreHired(r.Int(), heads: r.Int(), from: (Market)r.Int());
        }

        var waiting = r.Int();
        for (var i = 0; i < waiting; i++)
        {
            var id = r.Int();
            var group = world.Migration.Restore(id, r.Double(), r.Double(), r.Int(), r.Long(), null);
            var riding = r.Int();
            if (riding >= 0)
            {
                group.Riding = riding;
            }
        }

        var visits = r.Int();
        for (var i = 0; i < visits; i++)
        {
            var id = r.Int();
            var x = r.Double();
            var y = r.Double();
            var heads = r.Int();
            var market = (Market)r.Int();
            var since = r.Long();
            var until = r.Long();
            var riding = r.Int();
            var staying = r.Int();

            world.Tourism.Restore(
                id, x, y, heads, market, since, until,
                riding < 0 ? null : riding, staying < 0 ? null : staying);
        }

        var loans = r.Int();
        for (var i = 0; i < loans; i++)
        {
            world.Loans.Restore(
                (Market)r.Int(), r.Double(), r.Double(), r.Double(), r.Long(), r.Long());
        }

        var burnt = new List<Market>();
        var burntCount = r.Int();
        for (var i = 0; i < burntCount; i++)
        {
            burnt.Add((Market)r.Int());
        }

        world.Loans.RestoreHistory(burnt, r.Int(), r.Int());

        var tenders = r.Int();
        for (var i = 0; i < tenders; i++)
        {
            world.Contracts.Restore(
                r.Int(), (Market)r.Int(), r.Int(), r.Double(), r.Double(),
                r.Long(), r.Long(), r.Long(), (ContractState)r.Int(), r.Double());
        }

        world.Contracts.RestoreRelations(Market.East, r.Double());
        world.Contracts.RestoreRelations(Market.West, r.Double());

        var rules = r.Int();
        for (var i = 0; i < rules; i++)
        {
            world.TradeRules.Add(new TradeRule(r.Int(), (Market)r.Int(), (TradeAction)r.Int()));
        }

        var global = r.Int();
        world.BuildPolicy.SetGlobal(global < 0 ? null : global);
        var overrides = r.Int();
        for (var i = 0; i < overrides; i++)
        {
            var site = new Destination((DestinationKind)r.Int(), r.Int(), r.Double(), r.Double());
            var crossing = r.Int();
            world.BuildPolicy.SetSite(site, crossing < 0 ? null : crossing);
        }

        world.Buildings.SetNationalHours(r.Double());
        var kindRules = r.Int();
        for (var i = 0; i < kindRules; i++)
        {
            world.Buildings.SetKindHours(r.Int(), r.Double());
        }

        var entries = r.Int();
        for (var i = 0; i < entries; i++)
        {
            var tick = r.Long();
            world.Journal.Record(tick, new Command(
                (CommandKind)r.Int(),
                r.Int(), r.Int(), r.Int(),
                r.Double(), r.Double(), r.Double(), r.Double(), r.Double(),
                r.Bool(), r.String()));
        }

        return world;
    }

    /// <summary>Little-endian bits, and nothing clever.</summary>
    private sealed class Writer
    {
        private readonly List<byte> _bytes = [];

        internal void Bytes(ReadOnlySpan<byte> value) => _bytes.AddRange(value);

        internal void Bool(bool value) => _bytes.Add(value ? (byte)1 : (byte)0);

        internal void Int(int value)
        {
            Span<byte> buffer = stackalloc byte[4];
            BinaryPrimitives.WriteInt32LittleEndian(buffer, value);
            _bytes.AddRange(buffer);
        }

        internal void Long(long value)
        {
            Span<byte> buffer = stackalloc byte[8];
            BinaryPrimitives.WriteInt64LittleEndian(buffer, value);
            _bytes.AddRange(buffer);
        }

        internal void ULong(ulong value)
        {
            Span<byte> buffer = stackalloc byte[8];
            BinaryPrimitives.WriteUInt64LittleEndian(buffer, value);
            _bytes.AddRange(buffer);
        }

        /// <summary>
        /// The bit pattern, not a printed decimal. This is the whole reason the
        /// format is binary.
        /// </summary>
        internal void Double(double value) => ULong(BitConverter.DoubleToUInt64Bits(value));

        internal void String(string value)
        {
            var utf8 = System.Text.Encoding.UTF8.GetBytes(value);
            Int(utf8.Length);
            _bytes.AddRange(utf8);
        }

        internal byte[] ToArray() => [.. _bytes];
    }

    private sealed class Reader(byte[] bytes)
    {
        private readonly byte[] _bytes = bytes;
        private int _at;

        /// <summary>
        /// The next <paramref name="count"/> bytes, or a refusal.
        /// </summary>
        /// <remarks>
        /// <b>A truncated file is an input to decline, not a reason to bring
        /// the game down.</b> Only the first eight bytes used to be checked, so
        /// a save cut short by a full disk or a flipped bit sailed past the
        /// magic and the version and then threw an index-out-of-range out of
        /// the middle of a load — which reaches the player as a crash where the
        /// refusal path beside it exists to reach them as a sentence.
        /// </remarks>
        internal ReadOnlySpan<byte> Bytes(int count)
        {
            if (count < 0 || _at + count > _bytes.Length)
            {
                throw new InvalidDataException(
                    "that save is cut short — it ends part-way through a republic");
            }

            var span = _bytes.AsSpan(_at, count);
            _at += count;
            return span;
        }

        internal bool Bool() => Bytes(1)[0] != 0;

        internal int Int() => BinaryPrimitives.ReadInt32LittleEndian(Bytes(4));

        internal long Long() => BinaryPrimitives.ReadInt64LittleEndian(Bytes(8));

        internal ulong ULong() => BinaryPrimitives.ReadUInt64LittleEndian(Bytes(8));

        internal double Double() => BitConverter.UInt64BitsToDouble(ULong());

        internal string String()
        {
            var length = Int();
            if (length < 0)
            {
                throw new InvalidDataException("that save carries a string of negative length");
            }

            return System.Text.Encoding.UTF8.GetString(Bytes(length));
        }
    }
}
