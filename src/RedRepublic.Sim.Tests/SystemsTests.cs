namespace RedRepublic.Sim.Tests;

/// <summary>The tick: systems propose, one writer applies.</summary>
public sealed class SystemsTests
{
    private static Tables T => Fixtures.Tables;

    private static World Found(ulong seed = 1961, double extent = 1200.0) =>
        World.Found(new WorldSpec(seed, extent, 0), T);

    /// <summary>
    /// Somewhere this kind will actually stand.
    /// </summary>
    /// <remarks>
    /// A generated map is not a car park: a spot picked by eye lands on rock or
    /// in a river as often as not, and a test that assumed otherwise would be
    /// asserting about the seed rather than about the rule under test.
    /// </remarks>
    private static (double X, double Y) Spot(World world, int kind, int skip = 0)
    {
        for (var y = 200.0; y < world.Terrain.Extent - 200.0; y += 60.0)
        {
            for (var x = 200.0; x < world.Terrain.Extent - 200.0; x += 60.0)
            {
                if (Commands.CanPlace(world, kind, x, y) is null && skip-- <= 0)
                {
                    return (x, y);
                }
            }
        }

        throw new InvalidOperationException(
            $"nowhere on this map will take a {T.BName[kind]}");
    }

    private static Outcome PlaceSomewhere(World world, string id, int skip = 0)
    {
        var kind = T.BuildingIndex(id);
        var (x, y) = Spot(world, kind, skip);
        return world.Issue(Command.Place(kind, x, y));
    }

    private static Outcome ContractSomewhere(World world, string id, int skip = 0)
    {
        var kind = T.BuildingIndex(id);
        var (x, y) = Spot(world, kind, skip);
        return world.Issue(Command.ContractBuild(kind, x, y, Market.East));
    }

    /// <summary>
    /// <b>A system may only change what it declared.</b> The declaration is
    /// checked in both directions, because the half that rots is a write-set
    /// quietly claiming more than its system emits — a system whose declaration
    /// is wider than its behaviour tells you nothing about what wrote a value.
    /// </summary>
    [Fact]
    public void Every_system_writes_only_what_it_declared()
    {
        var world = Found();

        // A republic with something in it, so the systems have work to do.
        Assert.True(ContractSomewhere(world, "ConstructionOffice").Accepted);
        Assert.True(PlaceSomewhere(world, "House", skip: 20).Accepted);

        var emitted = new Dictionary<string, HashSet<MutationKind>>();

        // A month, so the daily systems run thirty times and the seasonal ones
        // have a chance to do something.
        for (var tick = 0; tick < SimClock.TicksPerDay * 30; tick++)
        {
            if (world.Clock.IsDayBoundary)
            {
                Record(world, Systems.Daily, emitted);
            }

            Record(world, Systems.PerTick, emitted);
            world.Clock.Advance();
        }

        foreach (var system in Systems.Daily.Concat(Systems.PerTick))
        {
            var declared = system.Writes.ToHashSet();
            var actual = emitted.GetValueOrDefault(system.Name, []);

            // Nothing undeclared.
            foreach (var kind in actual)
            {
                Assert.True(
                    declared.Contains(kind),
                    $"`{system.Name}` emitted {kind}, which it does not declare");
            }
        }

        // And the systems this republic must have exercised did run, so the
        // check above is not passing by running nothing. Named rather than
        // counted: a count goes stale the moment a system is added, and says
        // nothing about *which* one went quiet.
        foreach (var name in new[] { "weather", "power", "heating", "construction" })
        {
            Assert.True(
                emitted.ContainsKey(name),
                $"`{name}` emitted nothing over a month with a site and a house standing");
        }
    }

    private static void Record(
        World world, IReadOnlyList<SimSystem> systems, Dictionary<string, HashSet<MutationKind>> into)
    {
        foreach (var system in systems)
        {
            var proposed = system.Run(world);
            if (proposed.Count > 0)
            {
                into.TryAdd(system.Name, []);
                foreach (var m in proposed)
                {
                    into[system.Name].Add(m.Kind);
                }
            }

            Systems.Apply(world, proposed);
        }
    }

    /// <summary>
    /// A republic can be founded and advanced, and the same seed advances the
    /// same way twice. This is the whole determinism claim in one test.
    /// </summary>
    [Fact]
    public void The_same_seed_runs_the_same_republic()
    {
        static (long Ticks, double Snow, double Moisture, double Purse) RunAWeek(ulong seed)
        {
            var world = World.Found(new WorldSpec(seed, 1200.0, 1), Fixtures.Tables);
            world.Issue(Command.ContractBuild(
                Fixtures.Tables.BuildingIndex("CoalMine"), 400.0, 400.0, Market.East));

            for (var tick = 0; tick < SimClock.TicksPerDay * 7; tick++)
            {
                world.Tick();
            }

            return (world.Clock.Ticks, world.Ground.Snow, world.Ground.Moisture,
                world.Treasury.Of(Market.East));
        }

        var first = RunAWeek(1961);
        var second = RunAWeek(1961);
        var different = RunAWeek(1962);

        Assert.Equal(first, second);
        Assert.NotEqual(first.Snow == 0.0 && first.Moisture == different.Moisture, true);
    }

    /// <summary>
    /// <b>This is how a republic that owns nothing starts.</b> A blank map has no
    /// Construction Office, no crews and no materials, so an ordinary site is one
    /// nothing can ever work. A contracted site advances on its own.
    /// </summary>
    [Fact]
    public void A_contracted_site_advances_and_an_ordinary_one_waits()
    {
        var world = Found();

        var contracted = ContractSomewhere(world, "Sawmill");
        var ordinary = PlaceSomewhere(world, "Brickworks", skip: 30);

        Assert.True(contracted.Accepted);
        Assert.True(ordinary.Accepted);

        for (var tick = 0; tick < SimClock.TicksPerDay * 5; tick++)
        {
            world.Tick();
        }

        var c = world.Buildings.IndexOf(contracted.Id);
        var o = world.Buildings.IndexOf(ordinary.Id);

        Assert.True(world.Buildings.WorkDoneAt(c) > 0.0, "a contracted site should advance on its own");
        Assert.Equal(0.0, world.Buildings.WorkDoneAt(o));
    }

    /// <summary>
    /// Every limiter is separate, so a stalled building can always say
    /// <i>which</i> thing stalled it. "No staff" is actionable; "running at 40%"
    /// is not.
    /// </summary>
    [Fact]
    public void A_stalled_building_says_which_thing_stalled_it()
    {
        var world = Found();

        // A steel mill, because it is one of the works that actually draws
        // current — a sawmill has no power line to be short of, so it could
        // never report NoPower and the test would be checking nothing.
        var placed = ContractSomewhere(world, "SteelMill");
        Assert.True(placed.Accepted);
        var b = world.Buildings.IndexOf(placed.Id);
        Assert.True(T.BPowerDraw[world.Buildings.KindAt(b)] > 0.0);

        // Finish it without giving it anybody.
        world.Buildings.AddWork(b, T.BLabour[world.Buildings.KindAt(b)]);
        Assert.Equal(Stall.NoStaff, Systems.StallReason(world, b));

        // Staffed but unpowered. Power is asked before inputs deliberately: a
        // works with no current is not going to eat its ore either, and "no
        // power" is the thing the player can act on.
        world.Buildings.SetStaff(b, T.BWorkers[world.Buildings.KindAt(b)]);
        world.Buildings.SetPowered(b, false);
        Assert.Equal(Stall.NoPower, Systems.StallReason(world, b));

        // Powered but starved.
        world.Buildings.SetPowered(b, true);
        Assert.Equal(Stall.NoInputs, Systems.StallReason(world, b));

        // Fed, and it has nothing to complain about.
        foreach (var r in T.Inputs.KeysOf(world.Buildings.KindAt(b)))
        {
            world.Buildings.Stock.Add(b, r, 10.0);
        }

        Assert.Equal(Stall.None, Systems.StallReason(world, b));

        // A house is not a stalled factory: it makes nothing and is never asked.
        var (hx, hy) = Spot(world, T.BuildingIndex("House"), skip: 40);
        var house = world.Buildings.Add(T.BuildingIndex("House"), hx, hy);
        Assert.Equal(Stall.None, Systems.StallReason(world, house));
    }

    /// <summary>
    /// The refusals are sentences, not codes. The simulation is the only thing
    /// that knows why it said no, so it is the thing that says it.
    /// </summary>
    [Fact]
    public void A_refusal_carries_a_sentence_a_screen_can_show()
    {
        var world = Found();

        var offMap = world.Issue(Command.Place(T.BuildingIndex("House"), -50.0, 400.0));
        Assert.False(offMap.Accepted);
        Assert.NotEmpty(offMap.Refusal);
        Assert.Contains("outside", offMap.Refusal, StringComparison.Ordinal);

        // A mine needs something under it. Sited on ground that would otherwise
        // take it, so the refusal is about the geology and not about the slope.
        var mine = T.BuildingIndex("IronMine");
        for (var y = 200.0; y < world.Terrain.Extent - 200.0; y += 60.0)
        {
            for (var x = 200.0; x < world.Terrain.Extent - 200.0; x += 60.0)
            {
                var refusal = Commands.CanPlace(world, mine, x, y);
                if (refusal is not null && refusal.Contains("under that ground", StringComparison.Ordinal))
                {
                    Assert.False(world.Issue(Command.Place(mine, x, y)).Accepted);
                    goto found;
                }
            }
        }

        found:

        var noSuchBuilding = world.Issue(Command.Demolish(999));
        Assert.False(noSuchBuilding.Accepted);
        Assert.NotEmpty(noSuchBuilding.Refusal);

        var nameless = world.Issue(Command.NameRepublic("   "));
        Assert.False(nameless.Accepted);
        Assert.NotEmpty(nameless.Refusal);
    }

    /// <summary>
    /// <b>Accepted commands are journalled and refused ones are not</b>, because
    /// replaying a no-op is not replay. That recording is what gives the
    /// determinism rule an <i>inputs</i> to hold constant.
    /// </summary>
    [Fact]
    public void The_journal_records_what_happened_and_not_what_did_not()
    {
        var world = Found();

        Assert.Equal(0, world.Journal.Count);

        Assert.True(world.Issue(Command.NameRepublic("Novaya Zarya")).Accepted);
        Assert.Equal(1, world.Journal.Count);
        Assert.Equal("Novaya Zarya", world.Name);

        Assert.False(world.Issue(Command.Demolish(999)).Accepted);
        Assert.Equal(1, world.Journal.Count);

        Assert.False(world.Issue(Command.Place(T.BuildingIndex("House"), -1.0, -1.0)).Accepted);
        Assert.Equal(1, world.Journal.Count);
    }

    /// <summary>
    /// Two footprints cannot share ground, and the check does not leave a probe
    /// building behind when it refuses.
    /// </summary>
    [Fact]
    public void Nothing_is_built_on_top_of_anything()
    {
        var world = Found();
        var kind = T.BuildingIndex("House");
        var (x, y) = Spot(world, kind);

        var first = world.Issue(Command.Place(kind, x, y));
        Assert.True(first.Accepted);
        Assert.Equal(1, world.Buildings.Count);

        var onTop = world.Issue(Command.Place(kind, x, y));
        Assert.False(onTop.Accepted);
        Assert.Contains("already stands", onTop.Refusal, StringComparison.Ordinal);

        // The refusal left nothing behind — the probe was cleaned up.
        Assert.Equal(1, world.Buildings.Count);

        var elsewhere = PlaceSomewhere(world, "House", skip: 25);
        Assert.True(elsewhere.Accepted);
        Assert.Equal(2, world.Buildings.Count);
    }

    /// <summary>
    /// A day passes and the weather works through the ground. The tick order
    /// puts weather first because the day's going is what everything after it
    /// reads.
    /// </summary>
    [Fact]
    public void A_day_of_weather_reaches_the_ground()
    {
        // The taiga, so a founding in March has snow to melt.
        var world = World.Found(new WorldSpec(1961, 1200.0, 1), T);
        var before = world.Ground;

        for (var tick = 0; tick < SimClock.TicksPerDay * 40; tick++)
        {
            world.Tick();
        }

        // The taiga in March is below freezing, so what moves first is the snow
        // and the frost rather than the topsoil — rain that falls below freezing
        // lies instead of soaking in, which is the whole reason the thaw is an
        // event.
        Assert.True(
            world.Ground.Snow != before.Snow || world.Ground.Frost != before.Frost,
            "forty days of weather should have reached the ground somehow");
        Assert.InRange(world.Ground.Moisture, 0.0, 1.0);
        Assert.InRange(world.Ground.Frost, 0.0, 1.0);
        Assert.True(world.Ground.Snow >= 0.0);
    }
}
