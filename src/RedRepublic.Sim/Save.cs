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
    public const int Version = 1;

    public static byte[] Write(World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        var w = new Writer();

        w.Bytes(Magic);
        w.Int(Version);

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

        var entries = r.Int();
        for (var i = 0; i < entries; i++)
        {
            var tick = r.Long();
            world.Journal.Record(tick, new Command(
                (CommandKind)r.Int(),
                r.Int(), r.Int(), r.Int(),
                r.Double(), r.Double(), r.Double(),
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

        internal ReadOnlySpan<byte> Bytes(int count)
        {
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
            return System.Text.Encoding.UTF8.GetString(Bytes(length));
        }
    }
}
