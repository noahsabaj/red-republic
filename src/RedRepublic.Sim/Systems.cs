namespace RedRepublic.Sim;

/// <summary>
/// One pass over the republic: what it may change, and what it wants changed.
/// </summary>
/// <remarks>
/// The write-set is declared rather than inferred. A system that emits something
/// it did not declare is a system whose name no longer tells you what it does,
/// and <c>SystemsTests</c> checks the declaration in <b>both</b> directions —
/// the half that rots is a write-set quietly claiming more than its system
/// emits.
/// </remarks>
public sealed record SimSystem(string Name, MutationKind[] Writes, Func<World, List<Mutation>> Run);

/// <summary>
/// The tick: what happens, in what order, and why the order is that.
/// </summary>
/// <remarks>
/// <para>
/// <b>Systems propose, one writer applies.</b> No system touches the world
/// directly — each returns what it wants changed and <see cref="Apply"/> is the
/// only thing that carries it out. That is what makes "which system wrote this?"
/// answerable at all.
/// </para>
/// <para>
/// <b>Daily systems are daily for a reason.</b> People do not change jobs every
/// minute, and running the labour pass per tick cost 656 ms of simulated day at
/// only four thousand citizens — measured, not guessed. Contracts are daily for
/// a harder reason: deadlines are day indices, so running the sweep per tick
/// would fine a republic 1,440 times for one missed delivery.
/// </para>
/// </remarks>
public static class Systems
{
    /// <summary>
    /// The systems that run once a day, in order.
    /// </summary>
    /// <remarks>
    /// Weather first: the day's going is what everything after it reads.
    /// Contentment after labour, because it reads how many of a home's
    /// working-age residents hold a job and that is what labour has just
    /// decided. Demography after contentment, because a household decides
    /// whether to have a child by how the republic is treating it today.
    /// </remarks>
    public static IReadOnlyList<SimSystem> Daily { get; } =
    [
        new("weather", [MutationKind.Weather], Weather),
        new("sanitation", [MutationKind.Waste], Sanitation),
        new("pollution", [MutationKind.Pollution], Pollution),
        new("labour", [MutationKind.Staff, MutationKind.Employ, MutationKind.Consume], Labour),
        new("contracts", [MutationKind.Contract, MutationKind.Money], ContractsPass),
        new("loans", [MutationKind.Loan, MutationKind.Money], LoansPass),
        new("wages", [MutationKind.Money], Wages),
        new("contracting", [MutationKind.Money], Contracting),
        new("contentment", [MutationKind.Contentment], ContentmentPass),
        new("schooling", [MutationKind.Schooling], Schooling),
        new("demography", [MutationKind.Demography], Demography),
        new("migration", [MutationKind.Migration], MigrationPass),
        new("tourism", [MutationKind.Money], TourismPass),
        new("crews", [MutationKind.Crew], CrewsPass),
        new("morale", [MutationKind.Morale], Morale),
        new("tracks", [MutationKind.Wear], Tracks),
    ];

    /// <summary>
    /// The systems that run every tick, in order.
    /// </summary>
    /// <remarks>
    /// Power before everything that needs it, because a works with no current
    /// makes nothing. Construction before production so a site that opened this
    /// tick can work this tick rather than idling for a minute because two
    /// systems happened to be listed the other way round.
    /// </remarks>
    public static IReadOnlyList<SimSystem> PerTick { get; } =
    [
        new("power", [MutationKind.Powered, MutationKind.Lamps], Power),
        new("heating", [MutationKind.Heated, MutationKind.Consume], Heating),
        new("construction", [MutationKind.Build, MutationKind.BuildRoad, MutationKind.BuildLine, MutationKind.Consume], Construction),
        new("production", [MutationKind.Extract, MutationKind.Consume, MutationKind.Produce], Production),
        new("households", [MutationKind.Consume], Households),
        new("commissioning", [MutationKind.Commission], Commissioning),
        new("fleet", [MutationKind.Arrive, MutationKind.Transfer], FleetPass),
        new("dispatch", [MutationKind.Dispatch], Dispatch),
        new("trade", [MutationKind.Consume, MutationKind.Produce, MutationKind.Money], Trade),

        // Before dispatch: goods a belt has already moved are goods no lorry
        // needs to be sent for, and the other way round would send a lorry for a
        // load that was about to arrive on its own.
        new("belts", [MutationKind.Transfer], Belts),
        new("settling", [MutationKind.Migration, MutationKind.Dispatch], Settling),

        // After settling, and sharing its coaches: somebody who wants to live
        // here outranks somebody visiting, and the schedule is what says so.
        new("touring", [MutationKind.Tourism, MutationKind.Dispatch], Touring),
        new("clearing", [MutationKind.Ploughed], Clearing),
    ];

    /// <summary>
    /// Advance the republic one tick.
    /// </summary>
    public static List<Mutation> RunTick(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var all = new List<Mutation>();

        if (world.Clock.IsDayBoundary)
        {
            foreach (var system in Daily)
            {
                var proposed = system.Run(world);
                Apply(world, proposed);
                all.AddRange(proposed);
            }
        }

        foreach (var system in PerTick)
        {
            var proposed = system.Run(world);
            Apply(world, proposed);
            all.AddRange(proposed);
        }

        world.Clock.Advance();
        return all;
    }

    /// <summary>
    /// The one writer. Everything a system asked for happens here and nowhere
    /// else.
    /// </summary>
    public static void Apply(World world, IReadOnlyList<Mutation> mutations)
    {
        ArgumentNullException.ThrowIfNull(world);
        ArgumentNullException.ThrowIfNull(mutations);

        foreach (var m in mutations)
        {
            switch (m.Kind)
            {
                case MutationKind.Staff:
                    world.Buildings.SetStaff(m.Subject, (int)m.Amount);
                    break;

                case MutationKind.Powered:
                    world.Buildings.SetPowered(m.Subject, m.Amount > 0.0);
                    break;

                case MutationKind.Heated:
                    world.Buildings.SetHeated(m.Subject, m.Amount > 0.0);
                    break;

                case MutationKind.Lamps:
                    world.Roads.SetAlight(m.Subject, m.Amount > 0.0);
                    break;

                case MutationKind.BuildLine:
                    String(world, m.Subject, m.Amount);
                    break;

                case MutationKind.BuildRoad:
                    Lay(world, m.Subject, m.Amount);
                    break;

                case MutationKind.Extract:
                    world.Geology.Get(m.Subject)?.Extract(m.Amount);
                    world.Buildings.Stock.Add(m.Target, m.Resource, m.Amount);
                    break;

                case MutationKind.Consume:
                    world.Buildings.Stock.Take(m.Subject, m.Resource, m.Amount);
                    break;

                case MutationKind.Produce:
                    world.Buildings.Stock.Add(m.Subject, m.Resource, m.Amount);
                    break;

                case MutationKind.Build:
                    world.Buildings.AddWork(m.Subject, m.Amount);
                    break;

                case MutationKind.Contentment:
                case MutationKind.Morale:
                case MutationKind.Demography:
                case MutationKind.Schooling:
                    // Censuses rather than changes: the pass has already
                    // written what it decided, and the mutation records that
                    // it ran so the journal and a replay can see it.
                    break;

                case MutationKind.Commission:
                case MutationKind.Dispatch:
                case MutationKind.Arrive:
                case MutationKind.Transfer:
                    // The fleet moves itself: a journey is state rather than
                    // a change, and loading is one transaction the pass has
                    // to carry out where it decides it. The mutation records
                    // that it happened.
                    break;

                case MutationKind.Crew:
                case MutationKind.Migration:
                case MutationKind.Ploughed:
                    // Movements the pass carries out where it decides them;
                    // the mutation records that it happened.
                    break;

                case MutationKind.Weather:
                    // The ground carries itself; the mutation records that the
                    // day happened so a replay can see it.
                    break;

                case MutationKind.Money:
                    world.Treasury.Add((Market)m.Subject, m.Amount);
                    break;

                case MutationKind.Waste:
                    world.Buildings.Stock.Add(m.Subject, m.Resource, m.Amount);
                    break;

                case MutationKind.Pollution:
                    world.Lattice.Foul(m.Subject, m.Amount);
                    break;

                case MutationKind.Wear:
                    world.Lattice.WearIn(m.Subject, m.Amount);
                    break;

                default:
                    // Every kind the systems above can emit is handled. A kind
                    // that reaches here is one somebody added to the vocabulary
                    // without teaching the writer what it means, which is a
                    // build-time mistake rather than a runtime condition.
                    throw new NotSupportedException($"no writer for {m.Kind}");
            }
        }
    }

    /// <summary>
    /// Builder-days into a way, and the last of them opens it.
    /// </summary>
    /// <remarks>
    /// <b>A site is deliberately not in the network.</b> Nothing routes over it,
    /// nothing commutes along it, and no lorry is quicker for it existing —
    /// which is what makes the moment a road opens a real event. All of it lands
    /// in one transaction: the site stops existing, the crew is set down where
    /// its depot stood, and the way joins whichever network its grade carries. A
    /// finished railway is not a road that trains happen to use.
    /// </remarks>
    private static void Lay(World world, int site, double builderDays)
    {
        var road = world.RoadWorks.Get(site);
        if (road is null)
        {
            return;
        }

        var t = world.Tables;
        var labour = road.Labour(t);
        if (labour <= 0.0)
        {
            return;
        }

        var worked = Math.Min(builderDays, Math.Max(labour - road.WorkDone, 0.0));
        var share = worked / labour;
        var i = world.RoadWorks.IndexOf(site);

        // Materials go in step with the work, exactly as on a building site.
        for (var r = 0; r < t.Resources.Length; r++)
        {
            var wanted = road.Wants(r, t);
            if (wanted > 0.0)
            {
                world.RoadWorks.Stock.Take(i, r, wanted * share);
            }
        }

        road.WorkDone += worked;
        if (!road.IsBuilt(t))
        {
            return;
        }

        world.RoadWorks.Finish(road);
        world.Crews.Release(
            Destination.RoadSite(site), (road.FromX + road.ToX) / 2.0, (road.FromY + road.ToY) / 2.0);
        world.BuildPolicy.Forget(Destination.RoadSite(site));
        world.Reopen(road);
    }

    /// <summary>
    /// Builder-days into a line, and the last of them energises it.
    /// </summary>
    /// <remarks>
    /// One transaction, because the pieces must never land apart: the span
    /// leaves the works, the crew is set down where its depot stood, and the
    /// grid takes it over. A site removed without being energised is steel
    /// nobody can find; a span energised without the site being removed is one
    /// lorries keep delivering to. And everything already standing within reach
    /// is plugged in on the same breath — a line that came alive and connected
    /// nobody until the next placement would be a grid that woke for no reason
    /// the player could see.
    /// </remarks>
    private static void String(World world, int site, double builderDays)
    {
        var line = world.LineWorks.Get(site);
        if (line is null)
        {
            return;
        }

        var t = world.Tables;
        var labour = line.Labour(t);
        if (labour <= 0.0)
        {
            return;
        }

        var worked = Math.Min(builderDays, Math.Max(labour - line.WorkDone, 0.0));
        var share = worked / labour;
        var i = world.LineWorks.IndexOf(site);

        // Materials go in step with the work, exactly as on a building site: the
        // total a span eats over its life is its bill, rather than its bill being
        // demanded at every moment.
        foreach (var bill in t.Utilities[line.Kind].Materials)
        {
            world.LineWorks.Stock.Take(i, bill.Resource, bill.Tonnes * line.Kilometres * share);
        }

        line.WorkDone += worked;
        if (!line.IsBuilt(t))
        {
            return;
        }

        world.LineWorks.Finish(line);
        world.Crews.Release(
            Destination.LineSite(site), (line.FromX + line.ToX) / 2.0, (line.FromY + line.ToY) / 2.0);
        world.BuildPolicy.Forget(Destination.LineSite(site));
        world.Grid.Energise(line);
        world.Grid.AttachAlong(world.Buildings, line.Kind);
    }

    // ---- daily ----

    /// <summary>
    /// The day's weather, worked through the ground.
    /// </summary>
    /// <remarks>
    /// Temperature and rain are pure functions of the day, drawn from the
    /// world's own stream so a forecast can roll the same recurrence forward
    /// without perturbing anything.
    /// </remarks>
    private static List<Mutation> Weather(World world)
    {
        var day = world.Clock.DayOfYear;
        var temperature = Sim.Weather.TemperatureOn(world.Climate, day, world.Rng.NextDouble());
        var rain = Sim.Weather.PrecipitationOn(
            world.Climate, day, world.Rng.NextDouble(), world.Tables);

        var before = world.Ground.Snow;
        var ground = world.Ground;
        ground.Advance(temperature, rain, world.Tables);
        world.Ground = ground;

        // The day's fall is what buries the roads; when the pack has gone the
        // whole field is cleared rather than decayed, so a road ploughed last
        // February is not still credited for it next December.
        if (ground.Snow <= 0.0)
        {
            world.Lattice.Thaw();
        }
        else if (ground.Snow > before)
        {
            world.Lattice.Bury(Math.Clamp(
                (ground.Snow - before) / world.Tables.SnowBlocksMm, 0.0, 1.0));
        }

        return [new Mutation(MutationKind.Weather, 0, 0, -1, temperature, rain)];
    }

    /// <summary>What the republic throws away, and where it piles up.</summary>
    private static List<Mutation> Sanitation(World world)
    {
        var t = world.Tables;
        var waste = t.ResourceIndex("Waste");
        var mutations = new List<Mutation>();

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var kind = world.Buildings.KindAt(b);
            var perDay = t.BWaste[kind];
            if (perDay <= 0.0)
            {
                continue;
            }

            // On housing this is per resident, because what a block throws away
            // is a function of how many people live in it and not of how large
            // it is. On everything else it scales with how hard it is working —
            // an idle factory throws nothing away.
            var tonnes = t.BResidents[kind] > 0
                ? perDay * t.BResidents[kind]
                : perDay * world.Buildings.Activity(b);

            if (tonnes > 0.0)
            {
                mutations.Add(new Mutation(MutationKind.Waste, b, 0, waste, tonnes, 0.0));
            }
        }

        return mutations;
    }

    /// <summary>Smoke into the air, and a day of weather carrying it away.</summary>
    private static List<Mutation> Pollution(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var perDay = t.BPollution[world.Buildings.KindAt(b)];
            if (perDay <= 0.0)
            {
                continue;
            }

            var amount = perDay * world.Buildings.Activity(b);
            if (amount <= 0.0)
            {
                continue;
            }

            var cell = world.Lattice.CellOf(world.Buildings.XAt(b), world.Buildings.YAt(b));
            if (cell >= 0)
            {
                mutations.Add(Mutation.Pollution(cell, amount));
            }
        }

        // Proportional dispersal, applied here rather than emitted: it touches
        // every cell and a mutation per cell would be ten thousand entries a day
        // to say one thing.
        world.Lattice.Disperse(0.1);
        return mutations;
    }

    /// <summary>
    /// Who works where.
    /// </summary>
    /// <remarks>
    /// <b>Workplaces are filled in the plan's running order</b>, which is what
    /// says the mine is manned before the offices are — the difference between a
    /// republic that eats and one that does not. A job is only a job if there is
    /// a way to get to it, so the journey is decided here alongside the
    /// workplace.
    /// </remarks>
    private static List<Mutation> Labour(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();
        var order = world.Buildings.ByStanding();

        // Everyone available, and nobody employed until this pass says so.
        var free = new List<int>();
        for (var c = 0; c < world.Citizens.Count; c++)
        {
            if (world.Citizens.CanWork(c))
            {
                free.Add(c);
            }
        }

        var taken = new bool[world.Citizens.Count];

        // What the depots can carry today, fixed for the whole pass rather than
        // recomputed per workplace: it is one fleet serving the republic, not a
        // fresh allowance for every factory. Read off yesterday's staffing,
        // because a depot staffed yesterday is what runs buses this morning —
        // deriving it from the staffing this pass is about to decide would let a
        // depot carry its own drivers to work.
        var services = Transport.Services(world);
        var seatsUsed = 0;

        foreach (var b in order)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                mutations.Add(Mutation.Staff(b, 0));
                continue;
            }

            var wanted = world.Buildings.Jobs(b);
            var bar = (Education)t.BSchooling[world.Buildings.KindAt(b)];

            // Whether getting here means being out in the dark. A place running
            // more than a day shift needs a lit way in or a seat on something,
            // and that is the join between the roster and the street lamps:
            // without it, lighting a road would be a decoration nothing read.
            var dark = world.Buildings.WorksAfterDark(b);

            // Rank: walkers first, then by journey time, then by id. A seat spent
            // on somebody who could have walked is a seat denied to somebody who
            // could not.
            var candidates = new List<(int Rank, double Time, int Citizen, Commute How)>();
            foreach (var c in free)
            {
                if (taken[c] || world.Citizens.EducationAt(c) < bar)
                {
                    continue;
                }

                var home = world.Buildings.IndexOf(world.Citizens.HomeAt(c));
                if (home < 0)
                {
                    continue;
                }

                var how = Transport.ReachAt(
                    world,
                    world.Buildings.XAt(home), world.Buildings.YAt(home),
                    world.Buildings.XAt(b), world.Buildings.YAt(b),
                    services, dark);

                if (how is null)
                {
                    continue;
                }

                candidates.Add((how.Value.IsCarried ? 1 : 0, how.Value.Time, c, how.Value));
            }

            candidates.Sort((x, y) =>
            {
                var byRank = x.Rank.CompareTo(y.Rank);
                if (byRank != 0)
                {
                    return byRank;
                }

                var byTime = x.Time.CompareTo(y.Time);
                return byTime != 0 ? byTime : x.Citizen.CompareTo(y.Citizen);
            });

            var hired = 0;
            foreach (var (_, _, c, how) in candidates)
            {
                if (hired >= wanted)
                {
                    break;
                }

                if (how.IsCarried)
                {
                    var at = services.FindIndex(s => (int)s.Medium == how.Medium);

                    // That service is full. Everyone behind this candidate is
                    // either also a rider or ranked worse, so keep scanning
                    // rather than stopping — a nearer rider might still be a
                    // walker for a later workplace.
                    if (at < 0 || services[at].Seats <= 0)
                    {
                        continue;
                    }

                    services[at] = services[at] with { Seats = services[at].Seats - 1 };
                    seatsUsed++;
                }

                taken[c] = true;
                hired++;
                world.Citizens.SetWorkplace(c, world.Buildings.IdAt(b), how);
            }

            mutations.Add(Mutation.Staff(b, hired));
        }

        // Anyone the pass did not place holds no job today.
        for (var c = 0; c < world.Citizens.Count; c++)
        {
            if (!taken[c])
            {
                world.Citizens.SetWorkplace(c, -1, Commute.None);
            }
        }

        // And the depots burn for what they carried. A commute is an ongoing
        // cost against the same refinery output everything else wants.
        var fuel = t.ResourceIndex("Fuel");
        foreach (var (depot, tonnes) in Transport.FuelBurn(world, seatsUsed))
        {
            mutations.Add(Mutation.Consume(depot, fuel, tonnes));
        }

        return mutations;
    }

    private static List<Mutation> ContractsPass(World world)
    {
        var mutations = new List<Mutation>();
        foreach (var (market, fine) in world.Contracts.Settle(world.Clock.DayIndex))
        {
            mutations.Add(Mutation.Money(market, -fine));
        }

        return mutations;
    }

    private static List<Mutation> LoansPass(World world)
    {
        var mutations = new List<Mutation>();
        foreach (var loan in world.Loans.Overdue(world.Clock.DayIndex))
        {
            var fine = world.Loans.Default(loan);
            world.Contracts.Sour(loan.Market, world.Tables.DefaultRelations);
            mutations.Add(Mutation.Money(loan.Market, -fine));
        }

        return mutations;
    }

    /// <summary>
    /// A day's pay for hired foreign hands.
    /// </summary>
    /// <remarks>
    /// Only foreign labour costs money. The republic's own people cost it
    /// nothing at all, which is the whole argument for training them.
    /// </remarks>
    private static List<Mutation> Wages(World world)
    {
        var mutations = new List<Mutation>();
        foreach (var market in new[] { Market.East, Market.West })
        {
            var wage = world.Crews.DailyWage(market, world.Tables);
            if (wage > 0.0)
            {
                mutations.Add(Mutation.Money(market, -wage));
            }
        }

        return mutations;
    }

    /// <summary>
    /// How the republic is serving the people in each home.
    /// </summary>
    /// <remarks>
    /// Applied directly rather than emitted per home: it writes one value per
    /// housing block per day and a mutation each would be machinery for no gain.
    /// The census is counted in a single walk of the population, because asking
    /// each home in turn is a republic squared.
    /// </remarks>
    private static List<Mutation> ContentmentPass(World world)
    {
        var census = world.Citizens.CensusByHome();
        var homes = 0;

        foreach (var (homeId, row) in census)
        {
            var b = world.Buildings.IndexOf(homeId);
            if (b < 0)
            {
                continue;
            }

            homes++;

            // Work is the share of working-age residents holding a job — the
            // figure the labour pass has just decided.
            var work = row.WorkingAge == 0 ? 1.0 : (double)row.Employed / row.WorkingAge;
            world.Buildings.SetProvisioned(b, world.Buildings.ProvisionedAt(b));
            world.Buildings.SetContentment(b, new Contentment(
                world.Buildings.ProvisionedAt(b),
                world.Buildings.HeatedAt(b) ? 1.0 : WarmthNotNeeded(world),
                0.0,
                0.0,
                0.0,
                work,
                0.0,
                0.0,
                world.Buildings.ComfortedAt(b)));
        }

        return homes > 0 ? [new Mutation(MutationKind.Contentment, homes, 0, -1, 0.0, 0.0)] : [];
    }

    /// <summary>
    /// Warmth on a day too mild to need any.
    /// </summary>
    /// <remarks>
    /// <b>One on a warm day, always.</b> Heating demand follows today's
    /// temperature and never the month, so a July estate is not unhappy about a
    /// boiler nobody has lit — it simply is not being asked.
    /// </remarks>
    private static double WarmthNotNeeded(World world)
    {
        var temperature = world.Climate.MeanOn(world.Clock.DayOfYear);
        return Sim.Weather.HeatingRequired(temperature, world.Tables) ? 0.0 : 1.0;
    }

    /// <summary>A day of health and loyalty drifting toward what the home is like.</summary>
    private static List<Mutation> Morale(World world)
    {
        var t = world.Tables;
        var moved = 0;

        for (var c = 0; c < world.Citizens.Count; c++)
        {
            var b = world.Buildings.IndexOf(world.Citizens.HomeAt(c));
            if (b < 0)
            {
                continue;
            }

            var content = world.Buildings.ContentmentAt(b).Overall(t);

            // Loyalty follows contentment slowly, so one bad winter does not
            // empty a town and one good month does not fill it.
            var loyalty = world.Citizens.LoyaltyAt(c);
            world.Citizens.SetLoyalty(c, loyalty + ((content - loyalty) * t.LoyaltyDrift));

            // Health settles where the care does. Without a doctor in reach it
            // settles low rather than falling for ever.
            var health = world.Citizens.HealthAt(c);
            var target = Math.Max(t.HealthUnserved, content);
            world.Citizens.SetHealth(c, health + ((target - health) * t.HealthDrift));
            moved++;
        }

        return moved > 0 ? [new Mutation(MutationKind.Morale, moved, 0, -1, 0.0, 0.0)] : [];
    }

    /// <summary>A day of packing fading out of the ground.</summary>
    /// <remarks>
    /// Without this every line any lorry ever drove is permanent, and the map
    /// fills with the ghosts of routes nobody uses. A corridor has to be kept.
    /// </remarks>
    private static List<Mutation> Tracks(World world)
    {
        world.Lattice.Fade(world.Tables.WearFadePerDay);
        return [];
    }


    /// <summary>
    /// People take what they need off a shelf within reach.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>Within reach is the whole mechanic.</b> A republic with a full
    /// warehouse and no shop near the housing is a republic whose people go
    /// hungry, and that is the point: goods have to be somewhere, and getting
    /// them somewhere is what lorries are for.
    /// </para>
    /// <para>
    /// Shops are drawn on nearest first, ties by id, so two runs of the same
    /// republic empty the same shelves in the same order.
    /// </para>
    /// <para>
    /// Comforts are counted apart from wants. Falling short of food is a failure
    /// and falling short of televisions is a missed opportunity, and contentment
    /// applies them as a lift rather than as one more thing to be short of.
    /// </para>
    /// </remarks>
    private static List<Mutation> Households(World world)
    {
        var t = world.Tables;
        var taken = new List<Mutation>();
        var perTick = 1.0 / SimClock.TicksPerDay;

        var census = world.Citizens.CensusByHome();
        if (census.Count == 0)
        {
            return taken;
        }

        var shops = new List<int>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b) && t.Sells.LengthOf(world.Buildings.KindAt(b)) > 0)
            {
                shops.Add(b);
            }
        }

        if (shops.Count == 0)
        {
            return taken;
        }

        var alcohol = t.ResourceIndex("Alcohol");
        var wants = new[]
        {
            (Resource: t.ResourceIndex("Food"), PerHead: t.FoodPerCitizen),
            (Resource: t.ResourceIndex("Clothes"), PerHead: t.ClothesPerCitizen),
            (Resource: alcohol, PerHead: t.AlcoholPerCitizen),
            (Resource: t.ResourceIndex("Electronics"), PerHead: t.ElectronicsPerCitizen),
        };

        // Homes in id order, so the republic serves the same estates first every
        // run rather than in whatever order the census happened to be built.
        foreach (var homeId in census.Keys.Order())
        {
            var home = world.Buildings.IndexOf(homeId);
            if (home < 0 || census[homeId].Residents == 0)
            {
                continue;
            }

            var residents = census[homeId].Residents;
            var reachable = new List<(double Gap, int Shop)>();
            foreach (var shop in shops)
            {
                var gap = Units.Distance(
                    world.Buildings.XAt(shop), world.Buildings.YAt(shop),
                    world.Buildings.XAt(home), world.Buildings.YAt(home));
                if (gap <= t.ServiceRadius)
                {
                    reachable.Add((gap, shop));
                }
            }

            reachable.Sort((a, b) =>
            {
                var byGap = a.Gap.CompareTo(b.Gap);
                return byGap != 0
                    ? byGap
                    : world.Buildings.IdAt(a.Shop).CompareTo(world.Buildings.IdAt(b.Shop));
            });

            var met = 0.0;
            var wanted = 0.0;
            var comfortShare = 0.0;
            var comforts = 0;
            var drink = 0.0;

            foreach (var (resource, perHead) in wants)
            {
                var need = residents * perHead * perTick;
                var got = 0.0;
                var outstanding = need;

                foreach (var (_, shop) in reachable)
                {
                    if (outstanding <= 0.0)
                    {
                        break;
                    }

                    var off = Math.Min(world.Buildings.Stock.Get(shop, resource), outstanding);
                    if (off > 0.0)
                    {
                        outstanding -= off;
                        got += off;
                        taken.Add(Mutation.Consume(shop, resource, off));
                    }
                }

                var share = need > 0.0 ? Math.Clamp(got / need, 0.0, 1.0) : 1.0;
                if (t.ResourceIsComfort[resource])
                {
                    comfortShare += share;
                    comforts++;
                    if (resource == alcohol)
                    {
                        drink = share;
                    }
                }
                else
                {
                    wanted += need;
                    met += got;
                }
            }

            world.Buildings.SetProvisioned(
                home, wanted > 0.0 ? Math.Clamp(met / wanted, 0.0, 1.0) : 1.0);
            world.Buildings.SetComforted(home, comforts > 0 ? comfortShare / comforts : 0.0);
            world.Buildings.SetDrink(home, drink);
        }

        return taken;
    }

    /// <summary>
    /// What a contracted firm charges for a day's work.
    /// </summary>
    /// <remarks>
    /// Several times what your own crews cost, which is the entire argument for
    /// building a Construction Office and training people. A republic that never
    /// stops contracting is a republic spending its grant on what it could have
    /// done itself — and that is a decision the player gets to make badly.
    /// </remarks>
    private static List<Mutation> Contracting(World world)
    {
        var t = world.Tables;
        var owed = new double[2];

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var market = world.Buildings.ContractorAt(b);
            if (market < 0 || world.Buildings.IsBuilt(b))
            {
                continue;
            }

            owed[market] += t.ContractorDays * t.ContractorRate;
        }

        var mutations = new List<Mutation>();
        for (var m = 0; m < owed.Length; m++)
        {
            if (owed[m] > 0.0)
            {
                mutations.Add(Mutation.Money((Market)m, -owed[m]));
            }
        }

        return mutations;
    }

    /// <summary>
    /// Who is born and who dies.
    /// </summary>
    /// <remarks>
    /// <b>A household decides whether to have a child by how the republic is
    /// treating it today</b>, which is why this runs after contentment. Nobody
    /// outlives the oldest age, and a republic cannot grow into housing it has
    /// not built.
    /// </remarks>
    private static List<Mutation> Demography(World world)
    {
        var t = world.Tables;
        var today = world.Clock.DayIndex;
        var born = 0;
        var died = 0;

        // Backwards, so removing somebody does not skip the next.
        for (var c = world.Citizens.Count - 1; c >= 0; c--)
        {
            // Birthdays are spread over the year by id, because a cohort that
            // ages together dies together and that sawtooth is an artefact.
            if (world.Citizens.BirthdayAt(c) == today % SimClock.DaysPerYear)
            {
                world.Citizens.SetAge(c, world.Citizens.AgeAt(c) + 1);
            }

            var age = world.Citizens.AgeAt(c);
            var frailty = age >= t.Oldest
                ? 1.0
                : (1.0 - world.Citizens.HealthAt(c)) * 0.0004 * (age / 40.0);

            if (world.Rng.NextDouble() < frailty)
            {
                world.Citizens.RemoveAt(c);
                died++;
            }
        }

        foreach (var (homeId, row) in world.Citizens.CensusByHome())
        {
            var home = world.Buildings.IndexOf(homeId);
            if (home < 0 || row.WorkingAge < 2)
            {
                continue;
            }

            if (world.Buildings.ContentmentAt(home).Overall(t) < t.BirthsNeed)
            {
                continue;
            }

            if (row.Residents >= t.BResidents[world.Buildings.KindAt(home)])
            {
                continue;
            }

            var odds = row.WorkingAge / 2 * t.BirthsPerPairYear / SimClock.DaysPerYear;
            if (world.Rng.NextDouble() < odds)
            {
                world.Citizens.Add(homeId, 0, 0, 1.0, Citizens.ArrivingLoyalty);
                born++;
            }
        }

        return born + died > 0
            ? [new Mutation(MutationKind.Demography, born, died, -1, 0.0, 0.0)]
            : [];
    }

    /// <summary>
    /// A day at school, for anyone of school age with a place to go.
    /// </summary>
    /// <remarks>
    /// Attendance is what makes a school worth building: a republic that never
    /// builds one raises a generation that cannot run its own mines.
    /// </remarks>
    private static List<Mutation> Schooling(World world)
    {
        var t = world.Tables;
        var taught = 0;

        var schools = new List<(int Building, int Teaches, int Places)>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var teaches = t.BTeaches[world.Buildings.KindAt(b)];
            if (teaches >= 0 && world.Buildings.Staffing(b) > 0.0)
            {
                schools.Add((b, teaches, t.BWorkers[world.Buildings.KindAt(b)] * 12));
            }
        }

        if (schools.Count == 0)
        {
            return [];
        }

        var places = new int[schools.Count];
        for (var i = 0; i < schools.Count; i++)
        {
            places[i] = schools[i].Places;
        }

        for (var c = 0; c < world.Citizens.Count; c++)
        {
            var age = world.Citizens.AgeAt(c);
            var wantsSchool = age >= t.SchoolAgeFrom && age < t.SchoolAgeTo;
            var wantsUniversity = !wantsSchool
                && age >= t.UniversityAgeFrom && age < t.UniversityAgeTo
                && world.Citizens.EducationAt(c) == Education.Schooled;

            if (!wantsSchool && !wantsUniversity)
            {
                continue;
            }

            var home = world.Buildings.IndexOf(world.Citizens.HomeAt(c));
            if (home < 0)
            {
                continue;
            }

            var wanted = wantsUniversity ? 1 : 0;
            for (var i = 0; i < schools.Count; i++)
            {
                if (schools[i].Teaches != wanted || places[i] <= 0)
                {
                    continue;
                }

                var gap = Units.Distance(
                    world.Buildings.XAt(schools[i].Building),
                    world.Buildings.YAt(schools[i].Building),
                    world.Buildings.XAt(home), world.Buildings.YAt(home));

                if (gap > t.ServiceRadius)
                {
                    continue;
                }

                places[i]--;
                world.Citizens.AddSchoolDay(c);
                world.Citizens.SetStudying(c, wantsUniversity);
                taught++;
                break;
            }
        }

        return taught > 0 ? [new Mutation(MutationKind.Schooling, taught, 0, -1, 0.0, 0.0)] : [];
    }


    /// <summary>
    /// A garage takes delivery of the vehicles its establishment allows.
    /// </summary>
    /// <remarks>
    /// The establishment is authored on the building rather than as a list of
    /// kinds inside this pass, for the reason every other property is: a list in
    /// logic is a thing you must remember to edit, and what you forget lands
    /// silently in a fallback. A depot with three shifts of drivers still has
    /// only as many lorries as it owns.
    /// </remarks>
    private static List<Mutation> Commissioning(World world)
    {
        var t = world.Tables;
        var delivered = 0;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var kind = world.Buildings.KindAt(b);
            var establishment = t.Establishment.KeysOf(kind);
            var counts = t.Establishment.ValuesOf(kind);
            if (establishment.Length == 0)
            {
                continue;
            }

            var id = world.Buildings.IdAt(b);
            for (var v = 0; v < establishment.Length; v++)
            {
                var have = 0;
                foreach (var owned in world.Fleet.OfGarage(id))
                {
                    if (world.Fleet.KindAt(owned) == establishment[v])
                    {
                        have++;
                    }
                }

                for (var more = have; more < counts[v]; more++)
                {
                    world.Fleet.Commission(
                        establishment[v], id, world.Buildings.XAt(b), world.Buildings.YAt(b));
                    delivered++;
                }
            }
        }

        return delivered > 0
            ? [new Mutation(MutationKind.Commission, delivered, 0, -1, 0.0, 0.0)]
            : [];
    }

    /// <summary>
    /// What each place is short of, most urgent first.
    /// </summary>
    /// <remarks>
    /// <b>A site under construction, a works with an empty bin and a store with a
    /// standing order are the same question</b> — somewhere wants tonnage it has
    /// not got. Ranking them together is what makes freight a plan rather than
    /// three special cases, and the ranking is what a player is really deciding
    /// when they set a priority.
    /// </remarks>
    private static List<(Destination To, int Resource, double Tonnes, double Urgency)> Needs(World world)
    {
        var t = world.Tables;
        var wanted = new List<(Destination, int, double, double)>();

        // The ways and the lines first, so a run waiting on gravel is in the same
        // ranking as a factory waiting on ore rather than in a queue of its own.
        // A site the republic is waiting on outranks a topped-up works: the thing
        // is not there yet.
        foreach (var road in world.RoadWorks.Sites)
        {
            var i = world.RoadWorks.IndexOf(road.Id);
            var remaining = 1.0 - road.Progress(t);
            for (var r = 0; r < t.Resources.Length; r++)
            {
                var outstanding = (road.Wants(r, t) * remaining)
                    - world.RoadWorks.Stock.Get(i, r);
                if (outstanding > t.MinLoad)
                {
                    wanted.Add((Destination.RoadSite(road.Id), r, outstanding, 2.0));
                }
            }
        }

        foreach (var line in world.LineWorks.Sites)
        {
            var i = world.LineWorks.IndexOf(line.Id);
            var left = 1.0 - line.Progress(t);
            foreach (var bill in t.Utilities[line.Kind].Materials)
            {
                var outstanding = (bill.Tonnes * line.Kilometres * left)
                    - world.LineWorks.Stock.Get(i, bill.Resource);
                if (outstanding > t.MinLoad)
                {
                    wanted.Add((Destination.LineSite(line.Id), bill.Resource, outstanding, 2.0));
                }
            }
        }

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);

            if (!world.Buildings.IsBuilt(b))
            {
                // A contracted site brings its own; nothing should be driven there.
                if (world.Buildings.ContractorAt(b) >= 0)
                {
                    continue;
                }

                var res = t.Materials.KeysOf(kind);
                for (var i = 0; i < res.Length; i++)
                {
                    var outstanding = world.Buildings.MaterialOutstanding(b, res[i]);
                    if (outstanding > t.MinLoad)
                    {
                        // A site the republic is waiting on outranks a topped-up
                        // works: the building is not there yet.
                        wanted.Add((Destination.Building(world.Buildings.IdAt(b)), res[i], outstanding, 2.0));
                    }
                }

                continue;
            }

            // A works wants its inputs kept a few days ahead, so a lorry is sent
            // before the bin runs dry rather than after.
            var inputs = t.Inputs.KeysOf(kind);
            var rates = t.Inputs.ValuesOf(kind);
            for (var i = 0; i < inputs.Length; i++)
            {
                var target = rates[i] * t.ResupplyAtDays;
                var held = world.Buildings.Stock.Get(b, inputs[i]);
                if (held < target)
                {
                    var short_ = Math.Min(target - held, world.Buildings.IntakeCapacity(b, inputs[i]));
                    if (short_ > t.MinLoad)
                    {
                        // An empty bin is a stall; a low one is only a warning.
                        wanted.Add((
                            Destination.Building(world.Buildings.IdAt(b)),
                            inputs[i], short_, held <= 0.0 ? 3.0 : 1.0));
                    }
                }
            }

            // A shop or a terminal wants what it was told to keep. Nothing else
            // in the republic would ever deliver to a station: it consumes
            // nothing and sells nothing, so the standing order is its demand.
            foreach (var r in t.Sells.KeysOf(kind))
            {
                var target = t.BStorage[kind] * 0.5;
                var held = world.Buildings.Stock.Get(b, r);
                if (target - held > t.MinLoad)
                {
                    wanted.Add((Destination.Building(world.Buildings.IdAt(b)), r, target - held, 2.5));
                }
            }

            if (t.BStoresToOrder[kind])
            {
                for (var r = 0; r < t.Resources.Length; r++)
                {
                    var ordered = world.Buildings.Orders.Get(b, r);
                    var held = world.Buildings.Stock.Get(b, r);
                    if (ordered - held > t.MinLoad)
                    {
                        wanted.Add((Destination.Building(world.Buildings.IdAt(b)), r, ordered - held, 1.5));
                    }
                }
            }
        }

        // Most urgent first, ties by building id then resource, so two runs of
        // the same republic send the same lorry to the same place.
        wanted.Sort((a, b) =>
        {
            var byUrgency = b.Item4.CompareTo(a.Item4);
            if (byUrgency != 0)
            {
                return byUrgency;
            }

            var byKind = a.Item1.Kind.CompareTo(b.Item1.Kind);
            if (byKind != 0)
            {
                return byKind;
            }

            var byId = a.Item1.Id.CompareTo(b.Item1.Id);
            return byId != 0 ? byId : a.Item2.CompareTo(b.Item2);
        });

        return wanted;
    }

    /// <summary>
    /// Where a lorry pulls up for a consignee, or <c>null</c> if it is gone.
    /// </summary>
    /// <remarks>
    /// A site can finish or be pulled down while a lorry is on its way to it,
    /// which is why every caller has to be able to be told there is nowhere to go
    /// rather than being handed a plausible pair of coordinates.
    /// </remarks>
    internal static (double X, double Y)? Where(World world, Destination to)
    {
        switch (to.Kind)
        {
            case DestinationKind.Building:
                {
                    var b = world.Buildings.IndexOf(to.Id);
                    return b < 0 ? null : (world.Buildings.XAt(b), world.Buildings.YAt(b));
                }

            case DestinationKind.RoadSite:
                {
                    // The middle of the run, because that is where the work is.
                    var run = world.RoadWorks.Get(to.Id);
                    return run is null
                        ? null
                        : ((run.FromX + run.ToX) / 2.0, (run.FromY + run.ToY) / 2.0);
                }

            case DestinationKind.LineSite:
                {
                    var site = world.LineWorks.Get(to.Id);
                    return site is null
                        ? null
                        : ((site.FromX + site.ToX) / 2.0, (site.FromY + site.ToY) / 2.0);
                }

            default:
                return (to.X, to.Y);
        }
    }

    /// <summary>How much more of a material a consignee will take.</summary>
    private static double Room(World world, Destination to, int resource)
    {
        switch (to.Kind)
        {
            case DestinationKind.Building:
                {
                    var b = world.Buildings.IndexOf(to.Id);
                    return b < 0
                        ? 0.0
                        : world.Buildings.IntakeCapacity(b, resource)
                            - world.Buildings.Stock.Get(b, resource);
                }

            case DestinationKind.RoadSite:
                {
                    var run = world.RoadWorks.Get(to.Id);
                    if (run is null)
                    {
                        return 0.0;
                    }

                    var at = world.RoadWorks.IndexOf(run.Id);
                    var todo = 1.0 - run.Progress(world.Tables);
                    return (run.Wants(resource, world.Tables) * todo)
                        - world.RoadWorks.Stock.Get(at, resource);
                }

            case DestinationKind.LineSite:
                {
                    // Exactly what the work still to do needs, and no more: a site
                    // is not a warehouse.
                    var site = world.LineWorks.Get(to.Id);
                    if (site is null)
                    {
                        return 0.0;
                    }

                    var i = world.LineWorks.IndexOf(site.Id);
                    var left = 1.0 - site.Progress(world.Tables);
                    return (site.Wants(resource, world.Tables) * left)
                        - world.LineWorks.Stock.Get(i, resource);
                }

            default:
                return 0.0;
        }
    }

    /// <summary>Set a load down at a consignee, and say how much of it went in.</summary>
    private static double SetDown(World world, Destination to, int resource, double tonnes)
    {
        switch (to.Kind)
        {
            case DestinationKind.Building:
                {
                    var b = world.Buildings.IndexOf(to.Id);
                    if (b < 0)
                    {
                        return 0.0;
                    }

                    world.Buildings.Stock.Add(b, resource, tonnes);
                    return tonnes;
                }

            case DestinationKind.RoadSite:
                {
                    var at = world.RoadWorks.IndexOf(to.Id);
                    if (at < 0)
                    {
                        return 0.0;
                    }

                    world.RoadWorks.Stock.Add(at, resource, tonnes);
                    return tonnes;
                }

            case DestinationKind.LineSite:
                {
                    var i = world.LineWorks.IndexOf(to.Id);
                    if (i < 0)
                    {
                        return 0.0;
                    }

                    world.LineWorks.Stock.Add(i, resource, tonnes);
                    return tonnes;
                }

            default:
                return 0.0;
        }
    }

    /// <summary>
    /// Who has spare of a resource — the yard a lorry is sent to.
    /// </summary>
    /// <remarks>
    /// A supplier keeps what it needs for itself: a works is not stripped of the
    /// ore it is about to smelt to feed one further down the chain.
    /// </remarks>
    private static int Supplier(World world, int resource, int notThis, double atLeast)
    {
        var t = world.Tables;
        var best = -1;
        var bestSpare = atLeast;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (b == notThis || !world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var kind = world.Buildings.KindAt(b);
            var held = world.Buildings.Stock.Get(b, resource);
            if (held <= 0.0)
            {
                continue;
            }

            // What it keeps back for itself.
            var keep = 0.0;
            var inputs = t.Inputs.KeysOf(kind);
            var rates = t.Inputs.ValuesOf(kind);
            for (var i = 0; i < inputs.Length; i++)
            {
                if (inputs[i] == resource)
                {
                    keep = rates[i] * t.ResupplyAtDays;
                }
            }

            foreach (var r in t.Sells.KeysOf(kind))
            {
                if (r == resource)
                {
                    keep = Math.Max(keep, t.BStorage[kind] * 0.5);
                }
            }

            var spare = held - keep;
            if (spare > bestSpare)
            {
                bestSpare = spare;
                best = b;
            }
        }

        return best;
    }

    /// <summary>
    /// Send the idle lorries out.
    /// </summary>
    /// <remarks>
    /// <b>A vehicle takes the work, fuels for it and sets out in one decision.</b>
    /// A lorry that accepted a job without the fuel to finish it would strand
    /// itself somewhere with a load nobody can reach, which is the failure the
    /// dispatch-time check exists to prevent.
    /// </remarks>
    private static List<Mutation> Dispatch(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();

        var idle = new List<int>();
        for (var v = 0; v < world.Fleet.Count; v++)
        {
            if (world.Fleet.StateAt(v) == VehicleState.Idle
                && t.VRole[world.Fleet.KindAt(v)] == "Freight")
            {
                idle.Add(v);
            }
        }

        if (idle.Count == 0)
        {
            return mutations;
        }

        var needs = Needs(world);
        var claimed = new HashSet<(Destination, int)>();

        foreach (var v in idle)
        {
            foreach (var (consignee, resource, tonnes, _) in needs)
            {
                if (!claimed.Add((consignee, resource)))
                {
                    continue;
                }

                // A works is not sent to fetch from itself. Nothing else can be
                // both ends of a haul: a line site holds goods and consumes them,
                // so it never supplies anybody.
                var notThis = consignee.Kind == DestinationKind.Building
                    ? world.Buildings.IndexOf(consignee.Id)
                    : -1;
                var from = Supplier(world, resource, notThis, t.MinLoad);
                if (from < 0)
                {
                    continue;
                }

                var kind = world.Fleet.KindAt(v);
                var load = Math.Min(tonnes, t.VCapacity[kind]);

                var journey = Plan(
                    world, v,
                    world.Fleet.XAt(v), world.Fleet.YAt(v),
                    world.Buildings.XAt(from), world.Buildings.YAt(from));

                if (journey is null)
                {
                    continue;
                }

                world.Fleet.SetJob(v, Job.Haul(
                    world.Buildings.IdAt(from), consignee, resource, load));
                world.Fleet.SetJourney(v, journey);
                world.Fleet.SetState(v, VehicleState.Fetching);
                mutations.Add(new Mutation(
                    MutationKind.Dispatch, v, from, resource, load, 0.0));
                break;
            }
        }

        return mutations;
    }

    /// <summary>
    /// Move everything that is moving.
    /// </summary>
    /// <remarks>
    /// Arrival is decided by the leg's absolute end tick, so a lorry arrives when
    /// it was always going to and nothing drifts over a long haul.
    /// </remarks>
    private static List<Mutation> FleetPass(World world)
    {
        var t = world.Tables;
        var now = (double)world.Clock.Ticks;
        var mutations = new List<Mutation>();

        for (var v = 0; v < world.Fleet.Count; v++)
        {
            var journey = world.Fleet.JourneyAt(v);
            if (journey is null || world.Fleet.IsBogged(v))
            {
                continue;
            }

            if (!journey.LegDoneBy(now))
            {
                var (x, y) = journey.PositionAt(now);
                world.Fleet.SetPosition(v, x, y);
                continue;
            }

            if (!journey.OnLastLeg)
            {
                journey.Advance(world.Fleet.SpeedOfLeg(v, Drag(world)), t);
                continue;
            }

            // Arrived. What happens next depends which way it was pointed.
            world.Fleet.SetPosition(v, journey.DestinationX, journey.DestinationY);
            world.Fleet.SetJourney(v, null);
            mutations.Add(new Mutation(MutationKind.Arrive, v, 0, -1, 0.0, 0.0));

            var job = world.Fleet.JobAt(v);
            switch (world.Fleet.StateAt(v))
            {
                // A coach that has reached the post takes its party aboard and
                // sets off for the estate. Loading is one transaction where the
                // pass decides it: a party marked as riding a coach that never
                // took them would be people in two places at once.
                case VehicleState.Fetching when job.Kind is JobKind.Settle or JobKind.Tour:
                    {
                        var to = Where(world, job.To);
                        var onward = to is null
                            ? null
                            : Plan(world, v, world.Fleet.XAt(v), world.Fleet.YAt(v),
                                to.Value.X, to.Value.Y);

                        if (onward is null)
                        {
                            GoHome(world, v);
                            break;
                        }

                        if (job.Kind == JobKind.Settle)
                        {
                            world.Migration.Board(job.Subject, v);
                        }
                        else
                        {
                            world.Tourism.Board(job.Subject, v);
                        }

                        world.Fleet.SetJourney(v, onward);
                        world.Fleet.SetState(v, VehicleState.Delivering);
                        break;
                    }

                // And sets them down. The party stops existing as a party in the
                // same breath: one that arrived without being housed would be
                // people the republic had fetched and lost.
                case VehicleState.Delivering when job.Kind is JobKind.Settle or JobKind.Tour:
                    {
                        var home = world.Buildings.IndexOf(job.To.Id);
                        if (home >= 0 && job.Kind == JobKind.Settle)
                        {
                            var group = world.Migration.Get(job.Subject);
                            if (group is not null)
                            {
                                SettleInto(world, group.Heads, home);
                                world.Migration.Remove(group);
                                mutations.Add(new Mutation(
                                    MutationKind.Migration, 1, 0, -1, 0.0, 0.0));
                            }
                        }
                        else if (home >= 0)
                        {
                            world.Tourism.CheckIn(job.Subject, world.Buildings.IdAt(home));
                            mutations.Add(new Mutation(
                                MutationKind.Tourism, 1, 0, -1, 0.0, 0.0));
                        }

                        GoHome(world, v);
                        break;
                    }

                case VehicleState.Fetching:
                    {
                        var from = world.Buildings.IndexOf(job.From);
                        var to = Where(world, job.To);
                        if (from < 0 || to is null)
                        {
                            GoHome(world, v);
                            break;
                        }

                        var loaded = world.Buildings.Stock.Take(from, job.Resource, job.Tonnes);
                        world.Fleet.Cargo.Add(v, job.Resource, loaded);

                        var onward = Plan(
                            world, v, world.Fleet.XAt(v), world.Fleet.YAt(v), to.Value.X, to.Value.Y);

                        if (onward is null)
                        {
                            GoHome(world, v);
                            break;
                        }

                        world.Fleet.SetJourney(v, onward);
                        world.Fleet.SetState(v, VehicleState.Delivering);
                        break;
                    }

                case VehicleState.Delivering:
                    {
                        var carried = world.Fleet.Cargo.Get(v, job.Resource);
                        var room = Math.Max(0.0, Room(world, job.To, job.Resource));
                        var setDown = Math.Min(carried, room);
                        if (setDown > 0.0)
                        {
                            setDown = SetDown(world, job.To, job.Resource, setDown);
                            world.Fleet.Cargo.Take(v, job.Resource, setDown);
                            mutations.Add(Mutation.Transfer(v, job.To.Id, job.Resource, setDown));
                        }

                        GoHome(world, v);
                        break;
                    }

                default:
                    world.Fleet.SetState(v, VehicleState.Idle);
                    world.Fleet.SetJob(v, Job.None);
                    break;
            }
        }

        return mutations;
    }

    /// <summary>
    /// What the belts and the pipelines move, without a lorry and without a
    /// driver.
    /// </summary>
    /// <remarks>
    /// <para>
    /// What a belt buys is a haul that needs no vehicle, no driver and no diesel.
    /// What it costs is that it goes exactly where it was built and nowhere else.
    /// The trade against the fleet is the whole point: a mine feeding a plant
    /// four hundred metres away wants a belt, and a mine feeding six things
    /// scattered over a valley wants lorries.
    /// </para>
    /// <para>
    /// Throughput is a property of the <b>network</b> rather than of a span,
    /// because a belt is a belt however many sections it has: adding a kilometre
    /// makes it longer, not wider.
    /// </para>
    /// </remarks>
    private static List<Mutation> Belts(World world)
    {
        var t = world.Tables;
        var perTick = 1.0 / SimClock.TicksPerDay;
        var mutations = new List<Mutation>();

        for (var kind = 0; kind < t.Utilities.Length; kind++)
        {
            var carries = t.Utilities[kind].Carries;
            if (carries.Length == 0)
            {
                continue;
            }

            // What each network can still move this tick, and what this pass has
            // already taken out of each yard — so two consumers on one belt are
            // not both told the same tonne is theirs.
            var left = new Dictionary<int, double>();
            var lifted = new Dictionary<(int Yard, int Resource), double>();

            foreach (var resource in carries)
            {
                // Everyone on a network of this kind who wants this, emptiest
                // first, ties on id so the answer is reproducible.
                var wanting = new List<(double Cover, int Building, int Network)>();
                for (var b = 0; b < world.Buildings.Count; b++)
                {
                    if (!world.Buildings.IsBuilt(b) || !Consumes(t, world.Buildings.KindAt(b), resource))
                    {
                        continue;
                    }

                    var network = world.Grid.NetworkOf(world.Buildings.IdAt(b), kind);
                    if (network >= 0)
                    {
                        wanting.Add((CoverDays(world, b, resource), b, network));
                    }
                }

                wanting.Sort((x, y) =>
                {
                    var byCover = x.Cover.CompareTo(y.Cover);
                    return byCover != 0 ? byCover : x.Building.CompareTo(y.Building);
                });

                foreach (var (_, consumer, network) in wanting)
                {
                    if (!left.TryGetValue(network, out var allowance))
                    {
                        allowance = t.Utilities[kind].Throughput * perTick;
                    }

                    if (allowance <= 1e-12)
                    {
                        continue;
                    }

                    var room = world.Buildings.IntakeCapacity(consumer, resource)
                        - world.Buildings.Stock.Get(consumer, resource);
                    if (room <= 0.0)
                    {
                        continue;
                    }

                    // Whoever on the same network is holding it, fullest first —
                    // the opposite ranking from the consumers, and for the same
                    // reason: draw down the yard closest to blocking.
                    var from = -1;
                    var most = 0.0;
                    for (var y = 0; y < world.Buildings.Count; y++)
                    {
                        if (y == consumer || !world.Buildings.IsBuilt(y)
                            || world.Grid.NetworkOf(world.Buildings.IdAt(y), kind) != network)
                        {
                            continue;
                        }

                        var held = world.Buildings.Stock.Get(y, resource)
                            - lifted.GetValueOrDefault((y, resource));
                        if (held > most)
                        {
                            most = held;
                            from = y;
                        }
                    }

                    if (from < 0)
                    {
                        continue;
                    }

                    var moved = Math.Min(most, Math.Min(room, allowance));
                    if (moved <= 0.0)
                    {
                        continue;
                    }

                    left[network] = allowance - moved;
                    lifted[(from, resource)] = lifted.GetValueOrDefault((from, resource)) + moved;
                    world.Buildings.Stock.Take(from, resource, moved);
                    world.Buildings.Stock.Add(consumer, resource, moved);
                    mutations.Add(Mutation.Transfer(from, consumer, resource, moved));
                }
            }
        }

        return mutations;
    }

    /// <summary>How many days of cover a works has of one of its inputs.</summary>
    private static double CoverDays(World world, int b, int resource)
    {
        var kind = world.Buildings.KindAt(b);
        var inputs = world.Tables.Inputs.KeysOf(kind);
        var rates = world.Tables.Inputs.ValuesOf(kind);
        for (var i = 0; i < inputs.Length; i++)
        {
            if (inputs[i] == resource && rates[i] > 0.0)
            {
                return world.Buildings.Stock.Get(b, resource) / rates[i];
            }
        }

        return double.PositiveInfinity;
    }

    private static bool Consumes(Tables t, int kind, int resource)
    {
        foreach (var r in t.Inputs.KeysOf(kind))
        {
            if (r == resource)
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// Getting the settlers off the frontier and into a home.
    /// </summary>
    /// <remarks>
    /// <b>People wait at a post, and they give up.</b> A republic that advertises
    /// for settlers and cannot fetch them loses them, which is what makes a coach
    /// and a road to the post part of the cost of growing. Anybody who can walk
    /// to a bed walks — so the coaches only ever go out for the people who
    /// genuinely need fetching, and a republic whose estates are near its border
    /// needs fewer of them.
    /// </remarks>
    private static List<Mutation> Settling(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();

        // Housing a coach is already filling counts as spoken for: a group that
        // walked into rooms somebody else is being driven to would put both on
        // the street. A journey is a commitment, and a dispatcher that only reads
        // arrivals will make it twice.
        var booked = new Dictionary<int, int>();
        var coming = new HashSet<int>();
        for (var v = 0; v < world.Fleet.Count; v++)
        {
            var job = world.Fleet.JobAt(v);
            if (job.Kind != JobKind.Settle)
            {
                continue;
            }

            coming.Add(job.Subject);
            booked[job.To.Id] = booked.GetValueOrDefault(job.To.Id) + t.ArrivalParty;
        }

        var settled = 0;
        foreach (var group in world.Migration.Waiting.ToList())
        {
            if (coming.Contains(group.Id) || group.Riding is not null)
            {
                continue;
            }

            // The emptiest block with room, ties on id — an intake goes where
            // there is most room rather than filling one stairwell and leaving
            // the next estate empty.
            var home = Roomiest(world, booked, group.X, group.Y, t.MaxWalkM);
            if (home >= 0)
            {
                var moved = SettleInto(world, group.Heads, home);
                booked[world.Buildings.IdAt(home)] =
                    booked.GetValueOrDefault(world.Buildings.IdAt(home)) + moved;
                world.Migration.Remove(group);
                settled++;
                continue;
            }

            // Too far to walk. It wants a coach, and one that cannot make the
            // round trip does not set out — unlike a load of gravel, a party
            // stranded in a field is people the republic has lost the use of.
            var bed = Roomiest(world, booked, group.X, group.Y, double.PositiveInfinity);
            if (bed < 0)
            {
                continue;
            }

            var coach = Nearest(world, "Passenger", group.X, group.Y);
            if (coach < 0)
            {
                continue;
            }

            var out_ = Plan(
                world, coach, world.Fleet.XAt(coach), world.Fleet.YAt(coach), group.X, group.Y);
            if (out_ is null)
            {
                continue;
            }

            world.Fleet.SetJob(coach, Job.Settle(group.Id, world.Buildings.IdAt(bed)));
            world.Fleet.SetJourney(coach, out_);
            world.Fleet.SetState(coach, VehicleState.Fetching);
            booked[world.Buildings.IdAt(bed)] =
                booked.GetValueOrDefault(world.Buildings.IdAt(bed)) + t.ArrivalParty;
            mutations.Add(new Mutation(MutationKind.Dispatch, coach, bed, -1, 0.0, 0.0));
        }

        if (settled > 0)
        {
            mutations.Add(new Mutation(MutationKind.Migration, settled, 0, -1, 0.0, 0.0));
        }

        return mutations;
    }

    /// <summary>
    /// Getting the visitors from the post to their beds.
    /// </summary>
    /// <remarks>
    /// The same coaches the settlers ride, and deliberately after them: somebody
    /// who wants to live here outranks somebody visiting, and the order of these
    /// two passes is what says so.
    /// </remarks>
    private static List<Mutation> Touring(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();

        var coming = new HashSet<int>();
        for (var v = 0; v < world.Fleet.Count; v++)
        {
            var job = world.Fleet.JobAt(v);
            if (job.Kind == JobKind.Tour)
            {
                coming.Add(job.Subject);
            }
        }

        foreach (var visit in world.Tourism.Visits.ToList())
        {
            if (visit.StayingAt is not null || visit.Riding is not null || coming.Contains(visit.Id))
            {
                continue;
            }

            var hotel = Hotel(world, visit.Heads);
            if (hotel < 0)
            {
                continue;
            }

            var coach = Nearest(world, "Passenger", visit.X, visit.Y);
            if (coach < 0)
            {
                continue;
            }

            var out_ = Plan(
                world, coach, world.Fleet.XAt(coach), world.Fleet.YAt(coach), visit.X, visit.Y);
            if (out_ is null)
            {
                continue;
            }

            world.Fleet.SetJob(coach, Job.Tour(visit.Id, world.Buildings.IdAt(hotel)));
            world.Fleet.SetJourney(coach, out_);
            world.Fleet.SetState(coach, VehicleState.Fetching);
            mutations.Add(new Mutation(MutationKind.Dispatch, coach, hotel, -1, 0.0, 0.0));
        }

        return mutations;
    }

    /// <summary>
    /// Move a party of settlers into a block, and say how many fitted.
    /// </summary>
    /// <remarks>
    /// One function because the two ways in — carried by a coach, or walked —
    /// differ only in the journey, and a second copy of "how many fit, spread the
    /// ages" is a second place for the answer to drift. Ages are spread across
    /// working life so an intake is not a cohort that retires together, and they
    /// arrive schooled because they were taught somewhere else.
    /// </remarks>
    private static int SettleInto(World world, int heads, int home)
    {
        var room = world.Tables.BResidents[world.Buildings.KindAt(home)]
            - world.Citizens.ResidentsOf(world.Buildings.IdAt(home));
        var taken = Math.Clamp(heads, 0, Math.Max(room, 0));

        for (var i = 0; i < taken; i++)
        {
            world.Citizens.AddArrival(
                world.Buildings.IdAt(home), 20 + (world.Citizens.Count % 30));
        }

        return taken;
    }

    /// <summary>
    /// The home with the most room, within reach of a point. -1 if there is none.
    /// </summary>
    private static int Roomiest(
        World world, Dictionary<int, int> booked, double x, double y, double within)
    {
        var t = world.Tables;
        var best = -1;
        var mostRoom = 0;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            if (!world.Buildings.IsBuilt(b) || t.BResidents[kind] == 0)
            {
                continue;
            }

            if (Units.Distance(world.Buildings.XAt(b), world.Buildings.YAt(b), x, y) > within)
            {
                continue;
            }

            var id = world.Buildings.IdAt(b);
            var room = t.BResidents[kind]
                - world.Citizens.ResidentsOf(id)
                - booked.GetValueOrDefault(id);

            if (room > mostRoom)
            {
                mostRoom = room;
                best = b;
            }
        }

        return best;
    }

    /// <summary>A hotel with a bed free, or -1.</summary>
    private static int Hotel(World world, int heads)
    {
        var t = world.Tables;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            if (world.Buildings.IsBuilt(b) && t.BBeds[kind] >= heads)
            {
                return b;
            }
        }

        return -1;
    }

    /// <summary>The nearest idle vehicle of a role, or -1.</summary>
    private static int Nearest(World world, string role, double x, double y)
    {
        var t = world.Tables;
        var best = -1;
        var nearest = double.PositiveInfinity;

        for (var v = 0; v < world.Fleet.Count; v++)
        {
            if (world.Fleet.StateAt(v) != VehicleState.Idle
                || t.VRole[world.Fleet.KindAt(v)] != role)
            {
                continue;
            }

            var gap = Units.Distance(world.Fleet.XAt(v), world.Fleet.YAt(v), x, y);
            if (gap < nearest)
            {
                nearest = gap;
                best = v;
            }
        }

        return best;
    }

    /// <summary>Its work is over: it stands where it is with nothing to do.</summary>
    private static void Park(World world, int v)
    {
        world.Fleet.SetState(v, VehicleState.Idle);
        world.Fleet.SetJob(v, Job.None);
    }

    /// <summary>Point it back at its garage, or park it where it stands.</summary>
    private static void GoHome(World world, int v)
    {
        var garage = world.Buildings.IndexOf(world.Fleet.HomeAt(v));
        if (garage < 0)
        {
            Park(world, v);
            return;
        }

        var home = Plan(
            world, v,
            world.Fleet.XAt(v), world.Fleet.YAt(v),
            world.Buildings.XAt(garage), world.Buildings.YAt(garage));

        if (home is null)
        {
            Park(world, v);
            return;
        }

        world.Fleet.SetJourney(v, home);
        world.Fleet.SetState(v, VehicleState.Returning);
    }

    /// <summary>
    /// A journey from here to there.
    /// </summary>
    /// <remarks>
    /// <b>Over the road if there is one, across country if there is not.</b> That
    /// is the whole reason a road is worth building: the same lorry makes its own
    /// speed on tarmac and crawls over a field. A route that cannot be found at
    /// all is a straight run across country, because the ground is always there
    /// even when the road is not.
    /// </remarks>
    private static Journey? Plan(World world, int v, double fromX, double fromY, double toX, double toY)
    {
        var t = world.Tables;
        var kind = world.Fleet.KindAt(v);
        var medium = (Medium)t.VMedium[kind];
        var network = world.NetworkFor(medium);

        var start = network.NearestNode(fromX, fromY, t.RoadAccessM);
        var finish = network.NearestNode(toX, toY, t.RoadAccessM);

        var xs = new List<double> { fromX };
        var ys = new List<double> { fromY };
        var limits = new List<double>();

        if (start >= 0 && finish >= 0 && start != finish)
        {
            var route = network.RouteBetween(start, finish);
            if (route is not null)
            {
                // Onto the road...
                xs.Add(network.NodeX(route.Nodes[0]));
                ys.Add(network.NodeY(route.Nodes[0]));
                limits.Add(-1.0);

                // ...along it...
                for (var i = 1; i < route.Nodes.Count; i++)
                {
                    xs.Add(network.NodeX(route.Nodes[i]));
                    ys.Add(network.NodeY(route.Nodes[i]));
                    limits.Add(SpeedBetween(network, route.Nodes[i - 1], route.Nodes[i]));
                }
            }
        }

        // ...and off it again at the far end.
        xs.Add(toX);
        ys.Add(toY);
        limits.Add(-1.0);

        var first = Units.Distance(xs[0], ys[0], xs[1], ys[1]);
        var speed = limits[0] >= 0.0
            ? Math.Min(Units.KphToMps(t.VOnRoadKph[kind]), limits[0])
            : Units.KphToMps(t.VCrossCountryKph[kind]);

        return Journey.Begin(xs, ys, limits, world.Clock.Ticks, Journey.LegTicks(first, speed, t));
    }

    private static double SpeedBetween(Network network, int a, int b)
    {
        foreach (var s in network.Segments)
        {
            if ((s.From == a && s.To == b) || (s.From == b && s.To == a))
            {
                return s.Speed;
            }
        }

        return -1.0;
    }

    /// <summary>
    /// How much the going slows a vehicle today.
    /// </summary>
    /// <remarks>
    /// One figure for the whole republic, because it does not rain on half a
    /// ten-kilometre map. Where it is worse than average is a property of the
    /// surface, and that is on the lattice.
    /// </remarks>
    private static double Drag(World world) =>
        1.0 + (world.Ground.Softness * (world.Tables.MudDrag - 1.0));


    /// <summary>
    /// The customs houses work through the standing instructions.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>The order of the rules is the decision.</b> When throughput or money
    /// runs short the first rule is served first, which is why moving one up or
    /// down is a command of its own rather than a re-send of the whole policy.
    /// </para>
    /// <para>
    /// A rule only reaches goods that are <i>at</i> a customs house within range
    /// of a post. Trading west means hauling west — the post settles in its own
    /// bloc's money, and that is what makes the two currencies geographic rather
    /// than a dropdown.
    /// </para>
    /// </remarks>
    private static List<Mutation> Trade(World world)
    {
        var t = world.Tables;
        if (world.TradeRules.Count == 0)
        {
            return [];
        }

        var mutations = new List<Mutation>();

        // Customs houses, and how much each can still clear today.
        var houses = new List<(int Building, Market Bloc)>();
        var cleared = new List<double>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b) || world.Buildings.Staffing(b) <= 0.0)
            {
                continue;
            }

            if (t.BuildingIds[world.Buildings.KindAt(b)] != "Customs")
            {
                continue;
            }

            var post = world.Frontier.NearestCrossing(
                world.Buildings.XAt(b), world.Buildings.YAt(b), null);
            if (post is null)
            {
                continue;
            }

            var gap = Units.Distance(
                post.Value.X, post.Value.Y, world.Buildings.XAt(b), world.Buildings.YAt(b));
            if (gap > t.CustomsRange)
            {
                continue;
            }

            houses.Add((b, post.Value.Bloc));
            cleared.Add(t.CustomsThroughputPerDay / SimClock.TicksPerDay);
        }

        if (houses.Count == 0)
        {
            return mutations;
        }

        foreach (var rule in world.TradeRules)
        {
            for (var h = 0; h < houses.Count; h++)
            {
                var (building, bloc) = houses[h];
                if (bloc != rule.Market || cleared[h] <= 0.0)
                {
                    continue;
                }

                var price = rule.Market == Market.East
                    ? t.ResourcePriceEast[rule.Resource]
                    : t.ResourcePriceWest[rule.Resource];

                if (rule.Action == TradeAction.Sell)
                {
                    var held = world.Buildings.Stock.Get(building, rule.Resource);
                    var sold = Math.Min(held, cleared[h]);
                    if (sold <= 0.0)
                    {
                        continue;
                    }

                    cleared[h] -= sold;
                    mutations.Add(Mutation.Consume(building, rule.Resource, sold));
                    mutations.Add(Mutation.Money(rule.Market, sold * price));
                }
                else
                {
                    var room = world.Buildings.IntakeCapacity(building, rule.Resource)
                        - world.Buildings.Stock.Get(building, rule.Resource);
                    var affordable = price > 0.0
                        ? world.Treasury.Of(rule.Market) / price
                        : 0.0;
                    var bought = Math.Min(Math.Min(cleared[h], Math.Max(0.0, room)), affordable);
                    if (bought <= 0.0)
                    {
                        continue;
                    }

                    cleared[h] -= bought;
                    mutations.Add(Mutation.Produce(building, rule.Resource, bought));
                    mutations.Add(Mutation.Money(rule.Market, -bought * price));
                }
            }
        }

        return mutations;
    }

    /// <summary>
    /// Construction offices send gangs to the sites that need them.
    /// </summary>
    /// <remarks>
    /// <b>A crew is drawn off the office's own staff</b>, so an office nobody
    /// works at sends nobody. Sites are served in commissioning order, which
    /// decides which site gets a crew rather than which site gets the days —
    /// splitting one gang across four sites would finish none of them.
    /// </remarks>
    private static List<Mutation> CrewsPass(World world)
    {
        var t = world.Tables;
        var sent = 0;

        for (var office = 0; office < world.Buildings.Count; office++)
        {
            if (!world.Buildings.IsBuilt(office))
            {
                continue;
            }

            if (t.BuildingIds[world.Buildings.KindAt(office)] != "ConstructionOffice")
            {
                continue;
            }

            var officeId = world.Buildings.IdAt(office);
            var staff = world.Buildings.StaffAt(office);
            var out_ = world.Crews.Posted(officeId);
            var spare = staff - out_;
            if (spare < t.BuildersPerSite)
            {
                continue;
            }

            // The first site with no gang on it, in commissioning order.
            foreach (var (site, x, y) in SitesInOrder(world))
            {
                if (world.Crews.WorkingAt(site) is not null)
                {
                    continue;
                }

                var party = world.Crews.Send(officeId, t.BuildersPerSite, x, y);
                party.Working = site;
                sent++;
                break;
            }
        }

        return sent > 0 ? [new Mutation(MutationKind.Crew, sent, 0, -1, 0.0, 0.0)] : [];
    }

    /// <summary>
    /// Every site with its materials on hand, in commissioning order, and where a
    /// gang would stand to work it.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Shared by the construction pass and the crews pass because they have to
    /// agree on the queue: a crew posted to the third site while the first is
    /// being worked would make the commissioning order something the player can
    /// see and cannot rely on.
    /// </para>
    /// <para>
    /// Buildings and lines are ranked <b>together</b>. A building's id is its
    /// place in the order; a span carries the count as it stood when it was
    /// ordered, and ties go to the building, because the building holding that
    /// number was placed first. Ordering a line therefore takes its turn like
    /// anything else rather than jumping the queue or waiting behind every
    /// factory in the republic.
    /// </para>
    /// </remarks>
    private static List<(Destination Site, double X, double Y)> SitesInOrder(World world)
    {
        var queue = new List<(long Ordered, int Tie, Destination Site, double X, double Y)>();

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            // A site somebody else is being paid to build is not a site your
            // crews work, and it does not want your gravel either: you bought the
            // labour and the materials together, which is what makes a contractor
            // cost several times a builder-day.
            if (world.Buildings.IsBuilt(b) || world.Buildings.ContractorAt(b) >= 0)
            {
                continue;
            }

            if (!world.Buildings.HasMaterials(b))
            {
                continue;
            }

            var id = world.Buildings.IdAt(b);
            queue.Add((id, 0, Destination.Building(id),
                world.Buildings.XAt(b), world.Buildings.YAt(b)));
        }

        foreach (var road in world.RoadWorks.Sites)
        {
            if (!world.RoadWorks.HasMaterials(road))
            {
                continue;
            }

            // The middle of the run, because that is where the work is.
            queue.Add((road.Ordered, 1, Destination.RoadSite(road.Id),
                (road.FromX + road.ToX) / 2.0, (road.FromY + road.ToY) / 2.0));
        }

        foreach (var line in world.LineWorks.Sites)
        {
            if (!world.LineWorks.HasMaterials(line))
            {
                continue;
            }

            queue.Add((line.Ordered, 2, Destination.LineSite(line.Id),
                (line.FromX + line.ToX) / 2.0, (line.FromY + line.ToY) / 2.0));
        }

        queue.Sort(static (a, b) =>
        {
            var order = a.Ordered.CompareTo(b.Ordered);
            if (order != 0)
            {
                return order;
            }

            var tie = a.Tie.CompareTo(b.Tie);
            return tie != 0 ? tie : a.Site.Id.CompareTo(b.Site.Id);
        });

        var sites = new List<(Destination, double, double)>(queue.Count);
        foreach (var entry in queue)
        {
            sites.Add((entry.Site, entry.X, entry.Y));
        }

        return sites;
    }

    /// <summary>
    /// Who wants to come, and who is still waiting.
    /// </summary>
    /// <remarks>
    /// <b>People come to a republic that looks worth living in.</b> Nobody arrives
    /// for a place with nowhere to live and nothing to eat, and a party that
    /// waits too long at a post goes home — which is what makes a coach and a
    /// road to the frontier part of the cost of growing.
    /// </remarks>
    private static List<Mutation> MigrationPass(World world)
    {
        var t = world.Tables;
        var today = world.Clock.DayIndex;
        var arrived = 0;

        // Groups that ran out of patience.
        var gone = world.Migration.GiveUp(today, t);

        // How the republic looks from outside: the mean of what its homes are
        // like, and room to put anybody.
        var homes = 0;
        var content = 0.0;
        var room = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b) || t.BResidents[world.Buildings.KindAt(b)] == 0)
            {
                continue;
            }

            homes++;
            content += world.Buildings.ContentmentAt(b).Overall(t);
            room += t.BResidents[world.Buildings.KindAt(b)];
        }

        room -= world.Citizens.Count;

        if (homes > 0 && room > 0)
        {
            var appeal = content / homes;
            if (appeal >= t.ContentAttracts && world.Rng.NextDouble() < t.ArrivalOdds)
            {
                var post = world.Frontier.Crossings.Count > 0
                    ? world.Frontier.Crossings[
                        (int)world.Rng.NextBounded((ulong)world.Frontier.Crossings.Count)]
                    : default;

                if (post.Id > 0)
                {
                    world.Migration.Arrive(
                        post.X, post.Y, Math.Min(t.ArrivalParty, room), today);
                    arrived++;
                }
            }
        }

        return arrived + gone.Count > 0
            ? [new Mutation(MutationKind.Migration, arrived, gone.Count, -1, 0.0, 0.0)]
            : [];
    }

    /// <summary>
    /// Visitors from abroad, and the hard currency they spend.
    /// </summary>
    /// <remarks>
    /// A tourist is not a resident: they occupy a bed, spend foreign money and go
    /// home. What draws them is what the republic is like to look at rather than
    /// to live in, floored so that even a grim posting draws somebody.
    /// </remarks>
    private static List<Mutation> TourismPass(World world)
    {
        var t = world.Tables;
        var today = world.Clock.DayIndex;
        var mutations = new List<Mutation>();

        // Whoever is staying spends every day of it, in their own bloc's money.
        foreach (var visit in world.Tourism.Visits)
        {
            if (visit.StayingAt is not null)
            {
                mutations.Add(Mutation.Money(visit.Market, visit.SpendPerDay(t)));
            }
        }

        foreach (var leaving in world.Tourism.Departing(today))
        {
            world.Tourism.Leave(leaving);
        }

        // Beds nobody is in.
        var beds = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b))
            {
                beds += t.BBeds[world.Buildings.KindAt(b)];
            }
        }

        if (beds > world.Tourism.HeadsStaying()
            && world.Frontier.Crossings.Count > 0
            && world.Rng.NextDouble() < t.AppealFloor)
        {
            var post = world.Frontier.Crossings[
                (int)world.Rng.NextBounded((ulong)world.Frontier.Crossings.Count)];
            world.Tourism.Arrive(
                post.X, post.Y,
                Math.Min(t.TourParty, beds - world.Tourism.HeadsStaying()),
                post.Bloc, today, t);
        }

        return mutations;
    }

    /// <summary>
    /// Ploughs push the snow off what the republic drives on.
    /// </summary>
    /// <remarks>
    /// <b>Snow does not know where the roads are; the plough does.</b> That split
    /// is what makes clearing a road something you watch happen on the map rather
    /// than a number going down.
    /// </remarks>
    private static List<Mutation> Clearing(World world)
    {
        var t = world.Tables;
        if (world.Ground.Snow <= 0.0)
        {
            return [];
        }

        var swept = 0;
        for (var v = 0; v < world.Fleet.Count; v++)
        {
            if (t.VRole[world.Fleet.KindAt(v)] != "Plough" || world.Fleet.IsBogged(v))
            {
                continue;
            }

            var cell = world.Lattice.CellOf(world.Fleet.XAt(v), world.Fleet.YAt(v));
            if (cell >= 0 && world.Lattice.ClearedAt(cell) < 1.0 - t.PloughAt)
            {
                world.Lattice.Clear(cell);
                swept++;
            }
        }

        return swept > 0 ? [new Mutation(MutationKind.Ploughed, swept, 0, -1, 0.0, 0.0)] : [];
    }

    // ---- per tick ----

    /// <summary>
    /// Who the grid can feed.
    /// </summary>
    /// <remarks>
    /// Generation is summed and spent in the labour plan's running order, so
    /// when there is not enough to go round the mine keeps its current before
    /// the offices do — the same decision the labour pass makes about people.
    /// </remarks>
    private static List<Mutation> Power(World world)
    {
        var t = world.Tables;
        var wire = t.UtilityIndex("Power");

        // What each grid can supply, after what its own length loses.
        var available = new Dictionary<int, double>();
        var drawn = new Dictionary<int, double>();

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue; // a half-built plant generates nothing
            }

            var kind = world.Buildings.KindAt(b);
            if (t.BPowerOutput[kind] <= 0.0 || world.Buildings.Staffing(b) <= 0.0)
            {
                continue;
            }

            if (!IsFuelled(world, b, kind))
            {
                continue;
            }

            // A plant strung to nothing lights nothing — including itself, which
            // is correct: an isolated power station is a shed full of turbines
            // with nowhere to send the current.
            var network = world.Grid.NetworkOf(world.Buildings.IdAt(b), wire);
            if (network < 0)
            {
                continue;
            }

            var made = t.BPowerOutput[kind] * world.Buildings.Activity(b)
                * world.Grid.Efficiency(network, wire);
            available[network] = available.GetValueOrDefault(network) + made;
        }

        // The stations that actually serve anybody: built, staffed, and on a
        // grid. Collected once rather than searched per consumer.
        var stations = new List<(double X, double Y, int Network)>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            if (!world.Buildings.IsBuilt(b) || !t.BTransforms[kind]
                || world.Buildings.Staffing(b) <= 0.0)
            {
                continue;
            }

            var network = world.Grid.NetworkOf(world.Buildings.IdAt(b), wire);
            if (network >= 0)
            {
                stations.Add((world.Buildings.XAt(b), world.Buildings.YAt(b), network));
            }
        }

        var mutations = new List<Mutation>();

        foreach (var b in Consumers(world))
        {
            var draw = t.BPowerDraw[world.Buildings.KindAt(b)];
            if (draw <= 0.0)
            {
                mutations.Add(Mutation.Powered(b, true));
                continue;
            }

            var on = world.Buildings.IsBuilt(b) && Feed(
                Feeder(stations, world.Buildings.XAt(b), world.Buildings.YAt(b), t), draw);
            mutations.Add(Mutation.Powered(b, on));
        }

        // <b>The streets are served last, and that is a decision rather than an
        // accident of ordering.</b> A republic short of generation should put its
        // lamps out before it stops a factory: the works is what pays for the
        // lamps. It is also visible — a town that goes dark at the edges is a
        // power shortage a player can see from the camera.
        for (var s = 0; s < world.Roads.SegmentCount; s++)
        {
            if (!world.Roads.Segments[s].Lamps)
            {
                continue;
            }

            var draw = t.LampMwPerKm * (world.Roads.Segments[s].Length / 1000.0);

            // The middle of the stretch, because that is what a station has to
            // reach: a segment is at most one junction spacing long, so its
            // midpoint is never far from either end.
            var lit = Feed(
                Feeder(stations, world.Roads.MidpointX(s), world.Roads.MidpointY(s), t), draw);
            mutations.Add(Mutation.Lamps(s, lit));
        }

        return mutations;

        // The nearest station in reach of a point, and through it its grid.
        // Written once because two different consumers ask it: a building, and a
        // lit stretch of street. A lamp is wired to a station exactly as a
        // factory is, so it is the same query and must stay the same answer.
        static int Feeder(List<(double X, double Y, int Network)> stations, double x, double y, Tables t)
        {
            var best = -1;
            var bestGap = 0.0;
            foreach (var (sx, sy, network) in stations)
            {
                var gap = Units.Distance(sx, sy, x, y);
                if (gap > t.TransformerRange)
                {
                    continue;
                }

                if (best < 0 || gap < bestGap || (gap == bestGap && network < best))
                {
                    best = network;
                    bestGap = gap;
                }
            }

            return best;
        }

        bool Feed(int network, double draw)
        {
            if (network < 0)
            {
                return false;
            }

            var spent = drawn.GetValueOrDefault(network);
            if (spent + draw > available.GetValueOrDefault(network))
            {
                return false;
            }

            drawn[network] = spent + draw;
            return true;
        }
    }

    /// <summary>Whether a works has at least a little of everything it burns.</summary>
    private static bool IsFuelled(World world, int b, int kind)
    {
        var inputs = world.Tables.Inputs.KeysOf(kind);
        foreach (var r in inputs)
        {
            if (world.Buildings.Stock.Get(b, r) <= 0.0)
            {
                return false;
            }
        }

        return true;
    }

    /// <summary>Who the boilers can reach, on a day cold enough to need them.</summary>
    private static List<Mutation> Heating(World world)
    {
        var t = world.Tables;
        var temperature = world.Climate.MeanOn(world.Clock.DayOfYear);
        var demandFactor = Sim.Weather.HeatDemandFactor(temperature, t);

        if (demandFactor <= 0.0)
        {
            // Nothing is being asked for, so nothing is short of it.
            var warm = new List<Mutation>();
            for (var b = 0; b < world.Buildings.Count; b++)
            {
                warm.Add(Mutation.Heated(b, true));
            }

            return warm;
        }

        var main = t.UtilityIndex("Heat");
        var perTick = 1.0 / SimClock.TicksPerDay;

        // Heat is a <b>network</b> quantity, not a republic-wide one. A block is
        // warm only if a main runs past it and something on the same main is
        // burning coal — which is what makes district heating a town-scale
        // decision rather than a number that covers the map.
        var demand = new Dictionary<int, double>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var wanted = t.BHeat[world.Buildings.KindAt(b)];
            if (!world.Buildings.IsBuilt(b) || wanted <= 0.0)
            {
                continue;
            }

            var network = world.Grid.NetworkOf(world.Buildings.IdAt(b), main);
            if (network >= 0)
            {
                demand[network] = demand.GetValueOrDefault(network) + (wanted * demandFactor);
            }
        }

        var mutations = new List<Mutation>();
        var budget = new Dictionary<int, double>();

        for (var boiler = 0; boiler < world.Buildings.Count; boiler++)
        {
            var kind = world.Buildings.KindAt(boiler);
            if (!world.Buildings.IsBuilt(boiler) || t.BHeatOutput[kind] <= 0.0)
            {
                continue;
            }

            // A boiler house with no main out of it heats nothing but itself, and
            // the simulation says so rather than quietly warming the republic.
            var network = world.Grid.NetworkOf(world.Buildings.IdAt(boiler), main);
            if (network < 0)
            {
                continue;
            }

            // A boiler house is a building like any other: no crew or no
            // electricity for its pumps and it makes nothing. Exempting it would
            // mean a blackout in January quietly left the heating on.
            if (t.BPowerDraw[kind] > 0.0 && !world.Buildings.PoweredAt(boiler))
            {
                continue;
            }

            var capacity = t.BHeatOutput[kind] * world.Buildings.Activity(boiler);
            if (capacity <= 0.0)
            {
                continue;
            }

            // What survives the pipes. Heat leaks badly, so a main strung across
            // the map delivers a fraction of what the boiler burnt for — the
            // whole reason district heating is a town-scale thing.
            var kept = world.Grid.Efficiency(network, main);
            if (kept <= 0.0)
            {
                continue;
            }

            // Throttle to what is still wanted <b>at the far end of the main</b>.
            // A boiler serving a mild day does not burn a cold day's coal — but
            // it must burn enough to cover what the pipes lose. Throttling to
            // demand and taking the loss out afterwards leaves it short by
            // exactly the loss, every time.
            var throttle = Math.Clamp(
                demand.GetValueOrDefault(network) / (capacity * kept), 0.0, 1.0);
            if (throttle <= 0.0)
            {
                continue;
            }

            // And burn only in proportion to what it actually manages to make.
            var inputs = t.Inputs.KeysOf(kind);
            var rates = t.Inputs.ValuesOf(kind);
            var fuelFactor = 1.0;
            for (var i = 0; i < inputs.Length; i++)
            {
                var needed = rates[i] * perTick * world.Buildings.Activity(boiler) * throttle;
                if (needed > 0.0)
                {
                    fuelFactor = Math.Min(
                        fuelFactor,
                        Math.Clamp(world.Buildings.Stock.Get(boiler, inputs[i]) / needed, 0.0, 1.0));
                }
            }

            var running = throttle * fuelFactor;
            if (running <= 0.0)
            {
                continue;
            }

            for (var i = 0; i < inputs.Length; i++)
            {
                mutations.Add(Mutation.Consume(
                    boiler, inputs[i], rates[i] * perTick * world.Buildings.Activity(boiler) * running));
            }

            var made = capacity * running * kept;
            budget[network] = budget.GetValueOrDefault(network) + made;
            demand[network] = Math.Max(demand.GetValueOrDefault(network) - made, 0.0);
        }

        foreach (var b in Consumers(world))
        {
            var need = t.BHeat[world.Buildings.KindAt(b)] * demandFactor;
            if (!world.Buildings.IsBuilt(b) || need <= 0.0)
            {
                mutations.Add(Mutation.Heated(b, true));
                continue;
            }

            // A block with no main past it is cold, whatever the republic is
            // burning elsewhere. This is the state the whole network exists to
            // make representable, and it is why heating is no longer a number.
            var network = world.Grid.NetworkOf(world.Buildings.IdAt(b), main);
            if (network < 0)
            {
                mutations.Add(Mutation.Heated(b, false));
                continue;
            }

            // The epsilon is load-bearing: a boiler throttled to demand produces
            // exactly demand, but demand was accumulated by summing and this is
            // spent by subtracting, and those two orders do not round the same
            // way. Without slack the last block on the list comes up a few ulps
            // short and goes cold on mild days while a hard January is fine —
            // which is the wrong way round, and is what gives it away.
            var held = budget.GetValueOrDefault(network);
            var on = held >= need - 1e-9;
            if (on)
            {
                budget[network] = held - need;
            }

            mutations.Add(Mutation.Heated(b, on));
        }

        return mutations;
    }

    /// <summary>
    /// Builder-days worked into sites.
    /// </summary>
    /// <remarks>
    /// The bill is consumed <b>in step with the work</b>, so the total a site
    /// eats over its life is exactly its bill — an earlier shape demanded the
    /// whole bill at every moment and got through twice it.
    /// </remarks>
    private static List<Mutation> Construction(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b))
            {
                continue;
            }

            // A contracted site advances on its own: the firm brings its own
            // hands and its own materials, and bills the treasury daily.
            var contracted = world.Buildings.ContractorAt(b) >= 0;
            var crew = world.Crews.WorkingAt(Destination.Building(world.Buildings.IdAt(b)));
            if (!contracted && crew is null)
            {
                continue;
            }

            if (!contracted && !world.Buildings.HasMaterials(b))
            {
                continue;
            }

            // A crew works its heads in builder-days over a day, so a tick is
            // that spread across the day's ticks. A contracted site advances at
            // a fixed rate the firm sets rather than at the republic's pace.
            var heads = contracted ? t.ContractorDays : crew!.Heads;
            var builderDays = heads / (double)SimClock.TicksPerDay;

            var kind = world.Buildings.KindAt(b);
            var labour = t.BLabour[kind];
            if (labour <= 0.0)
            {
                continue;
            }

            var share = builderDays / labour;
            mutations.Add(Mutation.Build(b, builderDays));

            if (!contracted)
            {
                var res = t.Materials.KeysOf(kind);
                var qty = t.Materials.ValuesOf(kind);
                for (var i = 0; i < res.Length; i++)
                {
                    var eaten = qty[i] * share;
                    if (eaten > 0.0)
                    {
                        mutations.Add(Mutation.Consume(b, res[i], eaten));
                    }
                }
            }
        }

        // And the ways, worked by the same crews out of the same offices. A road
        // is a site with a bill of materials like any other: the gravel has to be
        // driven to it, and until the last builder-day lands nothing routes over
        // it.
        foreach (var run in world.RoadWorks.Sites)
        {
            if (world.Crews.WorkingAt(Destination.RoadSite(run.Id)) is not { } gang)
            {
                continue;
            }

            if (!world.RoadWorks.HasMaterials(run))
            {
                continue;
            }

            var todo = Math.Max(run.Labour(t) - run.WorkDone, 0.0);
            var worked = Math.Min(gang.Heads / (double)SimClock.TicksPerDay, todo);
            if (worked > 0.0)
            {
                mutations.Add(Mutation.BuildRoad(run.Id, worked));
            }
        }

        // And the lines, for exactly the same reason.
        foreach (var site in world.LineWorks.Sites)
        {
            if (world.Crews.WorkingAt(Destination.LineSite(site.Id)) is not { } gang)
            {
                continue;
            }

            if (!world.LineWorks.HasMaterials(site))
            {
                continue;
            }

            var left = Math.Max(site.Labour(t) - site.WorkDone, 0.0);
            var days = Math.Min(gang.Heads / (double)SimClock.TicksPerDay, left);
            if (days > 0.0)
            {
                mutations.Add(Mutation.BuildLine(site.Id, days));
            }
        }

        return mutations;
    }

    /// <summary>
    /// What the republic makes this tick.
    /// </summary>
    /// <remarks>
    /// Every limiter is separate and multiplies — staffing, the roster, power,
    /// inputs, the growing season. They are deliberately not folded together, so
    /// a stalled building can always say <i>which</i> thing stalled it.
    /// </remarks>
    private static List<Mutation> Production(World world)
    {
        var t = world.Tables;
        var mutations = new List<Mutation>();
        var perTick = 1.0 / SimClock.TicksPerDay;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var kind = world.Buildings.KindAt(b);
            var activity = world.Buildings.Activity(b);
            if (activity <= 0.0)
            {
                continue;
            }

            if (t.BPowerDraw[kind] > 0.0 && !world.Buildings.PoweredAt(b))
            {
                continue;
            }

            var outputs = t.Outputs.KeysOf(kind);
            var rates = t.Outputs.ValuesOf(kind);
            if (outputs.Length == 0)
            {
                continue;
            }

            // Inputs first: a works with an empty bin makes nothing, and the
            // shortfall is a stall rather than a smaller batch.
            var inputs = t.Inputs.KeysOf(kind);
            var draws = t.Inputs.ValuesOf(kind);
            var starved = false;
            for (var i = 0; i < inputs.Length; i++)
            {
                if (!world.Buildings.Stock.Has(b, inputs[i]))
                {
                    starved = true;
                    break;
                }
            }

            if (starved)
            {
                continue;
            }

            for (var i = 0; i < inputs.Length; i++)
            {
                mutations.Add(Mutation.Consume(b, inputs[i], draws[i] * activity * perTick));
            }

            var taps = t.BTaps[kind];
            for (var i = 0; i < outputs.Length; i++)
            {
                var tonnes = rates[i] * activity * perTick;
                if (tonnes <= 0.0)
                {
                    continue;
                }

                if (taps >= 0)
                {
                    // An extractor draws from the ground it stands on, and the
                    // draw and the fill are one transaction.
                    var deposit = world.Buildings.TappedAt(b);
                    if (deposit < 0)
                    {
                        continue;
                    }

                    mutations.Add(Mutation.Extract(deposit, b, outputs[i], tonnes));
                }
                else
                {
                    mutations.Add(Mutation.Produce(b, outputs[i], tonnes));
                }
            }
        }

        return mutations;
    }

    /// <summary>
    /// Everything that draws, in the labour plan's running order.
    /// </summary>
    /// <remarks>
    /// The same order people are allocated in, deliberately: when a republic is
    /// short, it should be short of the same things in the same order whether
    /// what ran out was hands or current.
    /// </remarks>
    private static List<int> Consumers(World world)
    {
        var order = world.Buildings.ByStanding();
        var seen = new HashSet<int>(order);
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!seen.Contains(b))
            {
                order.Add(b);
            }
        }

        return order;
    }

    /// <summary>
    /// Why a building is doing nothing.
    /// </summary>
    public static Stall StallReason(World world, int b)
    {
        ArgumentNullException.ThrowIfNull(world);
        var t = world.Tables;
        var kind = world.Buildings.KindAt(b);

        // Anything that makes something the republic depends on can stall, and a
        // boiler house or a bus depot makes something even though its outputs
        // are empty — heat and journeys are not tonnage.
        if (t.Outputs.LengthOf(kind) == 0
            && t.BPowerOutput[kind] <= 0.0
            && t.BHeatOutput[kind] <= 0.0
            && t.BSeats[kind] == 0)
        {
            return Stall.None;
        }

        if (world.Buildings.Staffing(b) <= 0.0)
        {
            return Stall.NoStaff;
        }

        if (t.BPowerDraw[kind] > 0.0 && !world.Buildings.PoweredAt(b))
        {
            return Stall.NoPower;
        }

        foreach (var r in t.Inputs.KeysOf(kind))
        {
            if (!world.Buildings.Stock.Has(b, r))
            {
                return Stall.NoInputs;
            }
        }

        return Stall.None;
    }
}
