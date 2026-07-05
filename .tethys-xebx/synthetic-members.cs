using System;
namespace Syn
{
    public delegate int Transform(int x);
    public class Widget
    {
        [Obsolete("old event")]
        public event EventHandler Changed;
        public event EventHandler Renamed { add { } remove { } }
        public delegate void Nested(string s);
        [Obsolete]
        public const int Max = 10;
        public static readonly string Tag = "w";
    }
}
