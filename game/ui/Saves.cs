using System;
using System.Collections.Generic;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>One republic on file.</summary>
public readonly record struct OnFile(string File, string Name, DateTime Written);

/// <summary>
/// Where a republic is written, and how it is read back.
/// </summary>
/// <remarks>
/// <para>
/// <b>Written to a temporary file and renamed.</b> A save written in place is a
/// save that a crash halfway through replaces with half a file — and the half a
/// player loses is the republic they have, not the one they had. The rename is
/// the one operation a filesystem will do atomically.
/// </para>
/// <para>
/// <b>The simulation owns the format.</b> Nothing here parses anything: it hands
/// bytes to <see cref="Save"/> and takes bytes back. A shell that understood the
/// format would be a second thing that had to agree about it.
/// </para>
/// </remarks>
public static class Saves
{
    /// <summary>
    /// Where saves live: the user data directory, which is writable in an
    /// exported build where the game's own folder may not be.
    /// </summary>
    public const string Folder = "user://republics";

    public static Error Write(string file, Sim.World republic)
    {
        ArgumentNullException.ThrowIfNull(republic);
        DirAccess.MakeDirRecursiveAbsolute(Folder);

        var path = $"{Folder}/{file}";
        var temporary = path + ".writing";

        using (var writing = FileAccess.Open(temporary, FileAccess.ModeFlags.Write))
        {
            if (writing is null)
            {
                return FileAccess.GetOpenError();
            }

            writing.StoreBuffer(Save.Write(republic));
        }

        // The rename is what makes this safe. Until it happens the old save is
        // untouched, and a crash costs the player nothing they had.
        return DirAccess.Open(Folder)?.Rename(temporary, path) ?? Error.Failed;
    }

    /// <summary>Read one back, or say why not.</summary>
    public static Sim.World? Read(string file, Tables tables, out string why)
    {
        why = "";
        using var reading = FileAccess.Open(file, FileAccess.ModeFlags.Read);
        if (reading is null)
        {
            // The engine's own error name is a symbol out of somebody else's
            // enum, not a sentence: "FileCantOpen" is what a player was shown
            // when a save would not open.
            why = MenuScreen.Trouble(FileAccess.GetOpenError());
            return null;
        }

        try
        {
            return Save.Read(reading.GetBuffer((long)reading.GetLength()), tables);
        }
        catch (System.IO.InvalidDataException refused)
        {
            // The sentence the simulation wrote, shown rather than swallowed.
            // A save this build cannot read is refused with a reason, because
            // the failure that costs a player their republic is the one that
            // half-works.
            why = refused.Message;
            return null;
        }
    }

    /// <summary>Every republic on file, newest first.</summary>
    public static List<OnFile> OnFile()
    {
        var found = new List<OnFile>();
        using var folder = DirAccess.Open(Folder);
        if (folder is null)
        {
            return found;
        }

        foreach (var name in folder.GetFiles())
        {
            if (!name.EndsWith(".rrsv", StringComparison.Ordinal))
            {
                continue;
            }

            var path = $"{Folder}/{name}";
            found.Add(new OnFile(
                path,
                name[..^5],
                DateTimeOffset.FromUnixTimeSeconds(
                    (long)FileAccess.GetModifiedTime(path)).LocalDateTime));
        }

        found.Sort((a, b) => b.Written.CompareTo(a.Written));
        return found;
    }
}
