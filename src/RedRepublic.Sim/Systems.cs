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
        new("labour", [MutationKind.Staff, MutationKind.Employ], Labour),
        new("contracts", [MutationKind.Contract, MutationKind.Money], ContractsPass),
        new("loans", [MutationKind.Loan, MutationKind.Money], LoansPass),
        new("wages", [MutationKind.Money], Wages),
        new("contracting", [MutationKind.Money], Contracting),
        new("contentment", [MutationKind.Contentment], ContentmentPass),
        new("schooling", [MutationKind.Schooling], Schooling),
        new("demography", [MutationKind.Demography], Demography),
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
        new("power", [MutationKind.Powered], Power),
        new("heating", [MutationKind.Heated], Heating),
        new("construction", [MutationKind.Build, MutationKind.Consume], Construction),
        new("production", [MutationKind.Extract, MutationKind.Consume, MutationKind.Produce], Production),
        new("households", [MutationKind.Consume], Households),
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

        foreach (var b in order)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                mutations.Add(Mutation.Staff(b, 0));
                continue;
            }

            var wanted = world.Buildings.Jobs(b);
            var bar = (Education)t.BSchooling[world.Buildings.KindAt(b)];
            var hired = 0;

            foreach (var c in free)
            {
                if (hired >= wanted)
                {
                    break;
                }

                if (taken[c] || world.Citizens.EducationAt(c) < bar)
                {
                    continue;
                }

                // A job is only a job if there is a way to get to it.
                var home = world.Buildings.IndexOf(world.Citizens.HomeAt(c));
                if (home < 0)
                {
                    continue;
                }

                var distance = Units.Distance(
                    world.Buildings.XAt(home), world.Buildings.YAt(home),
                    world.Buildings.XAt(b), world.Buildings.YAt(b));

                if (distance > t.MaxWalkM)
                {
                    // Beyond walking: it needs a seat, and seats are the bus
                    // depot's business. Until one carries them, this is not a
                    // job they can hold.
                    continue;
                }

                taken[c] = true;
                hired++;
                world.Citizens.SetWorkplace(c, world.Buildings.IdAt(b), Commute.OnFoot(distance, t));
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
        var generated = 0.0;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b))
            {
                generated += t.BPowerOutput[world.Buildings.KindAt(b)] * world.Buildings.Activity(b);
            }
        }

        var mutations = new List<Mutation>();
        var spare = generated;

        foreach (var b in Consumers(world))
        {
            var draw = t.BPowerDraw[world.Buildings.KindAt(b)];
            if (draw <= 0.0)
            {
                mutations.Add(Mutation.Powered(b, true));
                continue;
            }

            var fed = spare >= draw;
            if (fed)
            {
                spare -= draw;
            }

            mutations.Add(Mutation.Powered(b, fed));
        }

        return mutations;
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

        var produced = 0.0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b))
            {
                produced += t.BHeatOutput[world.Buildings.KindAt(b)] * world.Buildings.Activity(b);
            }
        }

        var mutations = new List<Mutation>();
        var spare = produced;

        foreach (var b in Consumers(world))
        {
            var wanted = t.BHeat[world.Buildings.KindAt(b)] * demandFactor;
            if (wanted <= 0.0)
            {
                mutations.Add(Mutation.Heated(b, true));
                continue;
            }

            var reached = spare >= wanted;
            if (reached)
            {
                spare -= wanted;
            }

            mutations.Add(Mutation.Heated(b, reached));
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
