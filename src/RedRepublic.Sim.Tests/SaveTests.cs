namespace RedRepublic.Sim.Tests;

/// <summary>
/// A save reloads bit-exact.
/// </summary>
/// <remarks>
/// <b>The round-trip test is what catches every missed field.</b> The archived
/// build proved that: a field left out of the format is invisible until somebody
/// compares two runs, and by then the republic that was lost is lost. So this
/// does not check "a save loads" — it checks that the republic that comes back
/// goes on to behave identically to the one that was written.
/// </remarks>
public sealed class SaveTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>A republic with something in it, so a save has something to lose.</summary>
    private static World Lived(int days = 20)
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 1), T);
        var kind = T.BuildingIndex("Apartment");

        var (x, y) = (0.0, 0.0);
        for (var py = 120.0; py < world.Terrain.Extent - 120.0 && y == 0.0; py += 40.0)
        {
            for (var px = 120.0; px < world.Terrain.Extent - 120.0; px += 40.0)
            {
                if (Commands.CanPlace(world, kind, px, py) is null)
                {
                    (x, y) = (px, py);
                    break;
                }
            }
        }

        world.Issue(Command.NameRepublic("Novaya Zarya"));
        var placed = world.Issue(Command.ContractBuild(kind, x, y, Market.East));
        var home = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(home, T.BLabour[kind]);
        world.Buildings.SetContractor(home, -1);
        world.Issue(Command.SetPriority(placed.Id, Priority.First));
        world.Treasury.Add(Market.East, 250_000.0);

        for (var i = 0; i < 25; i++)
        {
            world.Citizens.AddArrival(world.Buildings.IdAt(home), 20 + i);
        }

        for (var tick = 0; tick < SimClock.TicksPerDay * days; tick++)
        {
            world.Tick();
        }

        return world;
    }

    /// <summary>
    /// <b>The test that matters.</b> Save a lived-in republic, load it, run both
    /// forward another month, and require they arrive at the same place. A field
    /// the format forgot changes the future rather than the present, which is why
    /// comparing the loaded state alone would not find it.
    /// </summary>
    [Fact]
    public void A_loaded_republic_goes_on_to_behave_like_the_one_that_was_saved()
    {
        var original = Lived();
        var bytes = Save.Write(original);
        var loaded = Save.Read(bytes, T);

        static (long Ticks, int People, double Purse, double Snow, double Moisture,
            double Frost, int Buildings, double Provisioned, int Schooled) Fingerprint(World w)
        {
            var provisioned = 0.0;
            for (var b = 0; b < w.Buildings.Count; b++)
            {
                provisioned += w.Buildings.ProvisionedAt(b);
            }

            return (
                w.Clock.Ticks,
                w.Citizens.Count,
                w.Treasury.Of(Market.East),
                w.Ground.Snow,
                w.Ground.Moisture,
                w.Ground.Frost,
                w.Buildings.Count,
                provisioned,
                w.Citizens.ByEducation(Education.Schooled));
        }

        // Identical the moment it comes back...
        Assert.Equal(Fingerprint(original), Fingerprint(loaded));
        Assert.Equal(original.Name, loaded.Name);

        // ...and still identical a month later, which is the half that catches a
        // missed field.
        for (var tick = 0; tick < SimClock.TicksPerDay * 30; tick++)
        {
            original.Tick();
            loaded.Tick();
        }

        Assert.Equal(Fingerprint(original), Fingerprint(loaded));
    }

    /// <summary>
    /// <b>The generator's position goes in, not its seed.</b> A save that restores
    /// the seed alone resumes a different future, and nothing complains.
    /// </summary>
    [Fact]
    public void The_generator_resumes_where_it_left_off()
    {
        var world = Lived(days: 5);
        var loaded = Save.Read(Save.Write(world), T);

        // The stream is wound to the same place, so the next thousand draws
        // agree.
        for (var i = 0; i < 1000; i++)
        {
            Assert.Equal(world.Rng.NextULong(), loaded.Rng.NextULong());
        }

        // And a republic founded on the same seed but not wound forward does
        // not — which is what makes the check above mean something.
        var fresh = World.Found(new WorldSpec(1961, 1500.0, 1), T);
        Assert.NotEqual(fresh.Rng.NextULong(), loaded.Rng.NextULong());
    }

    /// <summary>
    /// Ids survive the crossing. A save that renumbered its buildings would break
    /// every journal entry and every standing order that names one — and a
    /// citizen's birthday is derived from their id, so renumbering people would
    /// quietly move when they age.
    /// </summary>
    [Fact]
    public void Ids_come_back_as_they_were()
    {
        var world = Lived(days: 3);
        var loaded = Save.Read(Save.Write(world), T);

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            Assert.Equal(world.Buildings.IdAt(b), loaded.Buildings.IdAt(b));
        }

        for (var c = 0; c < world.Citizens.Count; c++)
        {
            Assert.Equal(world.Citizens.IdAt(c), loaded.Citizens.IdAt(c));
            Assert.Equal(world.Citizens.BirthdayAt(c), loaded.Citizens.BirthdayAt(c));
        }

        // And the next building minted after a load does not collide with one
        // the save already carried.
        var next = loaded.Buildings.Add(T.BuildingIndex("House"), 100.0, 100.0);
        for (var b = 0; b < loaded.Buildings.Count - 1; b++)
        {
            Assert.NotEqual(loaded.Buildings.IdAt(next), loaded.Buildings.IdAt(b));
        }
    }

    /// <summary>
    /// The journal is how the republic came to be, so it goes in the save. A
    /// republic that forgot its own history could not be replayed.
    /// </summary>
    [Fact]
    public void The_journal_survives()
    {
        var world = Lived(days: 2);
        var loaded = Save.Read(Save.Write(world), T);

        Assert.True(world.Journal.Count > 0);
        Assert.Equal(world.Journal.Count, loaded.Journal.Count);

        for (var i = 0; i < world.Journal.Count; i++)
        {
            Assert.Equal(world.Journal.Entries[i].Tick, loaded.Journal.Entries[i].Tick);
            Assert.Equal(world.Journal.Entries[i].Command, loaded.Journal.Entries[i].Command);
        }

        // Including the name, which is an input like any other.
        Assert.Contains(
            loaded.Journal.Entries,
            e => e.Command.Kind == CommandKind.NameRepublic && e.Command.Text == "Novaya Zarya");
    }

    /// <summary>
    /// <b>Every double comes back bit-exact.</b> This is the requirement that
    /// disqualified JSON: a parser that is not correctly rounded returns a
    /// different value for a great many doubles, and a republic one ulp out
    /// diverges from the one it was.
    /// </summary>
    [Fact]
    public void Every_double_survives_bit_exactly()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var kind = T.BuildingIndex("OpenYard");

        var (x, y) = (0.0, 0.0);
        for (var py = 120.0; py < world.Terrain.Extent - 120.0 && y == 0.0; py += 40.0)
        {
            for (var px = 120.0; px < world.Terrain.Extent - 120.0; px += 40.0)
            {
                if (Commands.CanPlace(world, kind, px, py) is null)
                {
                    (x, y) = (px, py);
                    break;
                }
            }
        }

        var placed = world.Issue(Command.ContractBuild(kind, x, y, Market.East));
        var b = world.Buildings.IndexOf(placed.Id);

        // Values chosen to be awkward: a repeating binary fraction, a
        // denormal-ish tiny, and one that no short decimal represents.
        var awkward = new[] { 0.1, 1.0 / 3.0, 5e-324, 1e308, Math.PI, 0.30000000000000004 };
        for (var i = 0; i < awkward.Length && i < T.Resources.Length; i++)
        {
            world.Buildings.Stock.Set(b, i, awkward[i]);
        }

        world.Treasury.Set(Market.West, Math.E * 1e17);

        var loaded = Save.Read(Save.Write(world), T);
        var lb = loaded.Buildings.IndexOf(placed.Id);

        for (var i = 0; i < awkward.Length && i < T.Resources.Length; i++)
        {
            Assert.Equal(awkward[i], loaded.Buildings.Stock.Get(lb, i));
            Assert.Equal(
                BitConverter.DoubleToUInt64Bits(awkward[i]),
                BitConverter.DoubleToUInt64Bits(loaded.Buildings.Stock.Get(lb, i)));
        }

        Assert.Equal(
            BitConverter.DoubleToUInt64Bits(Math.E * 1e17),
            BitConverter.DoubleToUInt64Bits(loaded.Treasury.Of(Market.West)));
    }

    /// <summary>
    /// A file that is not a save, or is one this build cannot read, is refused
    /// with a sentence. The failure that costs a player their republic is the one
    /// that half-works.
    /// </summary>
    [Fact]
    public void A_file_this_build_cannot_read_is_refused()
    {
        Assert.Throws<InvalidDataException>(
            () => Save.Read([0x00, 0x01, 0x02, 0x03, 0, 0, 0, 0], T));

        // A save from another version: right magic, wrong number.
        var bytes = Save.Write(World.Found(new WorldSpec(1, 1200.0, 0), T));
        bytes[4] = 99;
        var refused = Assert.Throws<InvalidDataException>(() => Save.Read(bytes, T));
        Assert.Contains("99", refused.Message, StringComparison.Ordinal);
    }
}
