using App.Models;

namespace App.Services
{
    public class UserService
    {
        public User Create(string name)
        {
            var u = new User();
            u.Describe();
            return u;
        }
    }
}
